// Package tui provides the pin queue screen and operations.
// Entrypoint: pinView.
package tui

import (
	"fmt"
	"os"
	"os/exec"
	"strings"

	"github.com/atotto/clipboard"
	"github.com/charmbracelet/bubbles/key"
	"github.com/charmbracelet/bubbles/table"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/huh"
	"github.com/charmbracelet/lipgloss"
	"github.com/google/shlex"
	"github.com/mitchfultz/ralph/ralph_tui/internal/config"
	"github.com/mitchfultz/ralph/ralph_tui/internal/paths"
	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
)

type pinMode int

const (
	pinModeTable pinMode = iota
	pinModeBlockForm
	pinModeMoveCheckedForm
)

type pinFocus int

const (
	pinFocusTable pinFocus = iota
	pinFocusDetail
)

type pinSection int

const (
	pinSectionQueue pinSection = iota
	pinSectionBlocked
)

func (s pinSection) Label() string {
	switch s {
	case pinSectionBlocked:
		return "Blocked"
	case pinSectionQueue:
		fallthrough
	default:
		return "Queue"
	}
}

type pinTableEntry struct {
	Section       pinSection
	ID            string
	Header        string
	Lines         []string
	Checked       bool
	FixupAttempts int
	WIPBranch     string
	KnownGood     string
	FixupLast     string
}

func queueEntry(item pin.QueueItem) pinTableEntry {
	return pinTableEntry{
		Section: pinSectionQueue,
		ID:      item.ID,
		Header:  item.Header,
		Lines:   item.Lines,
		Checked: item.Checked,
	}
}

func blockedEntry(item pin.BlockedItem) pinTableEntry {
	return pinTableEntry{
		Section:       pinSectionBlocked,
		ID:            item.ID,
		Header:        item.Header,
		Lines:         item.Lines,
		FixupAttempts: item.FixupAttempts,
		WIPBranch:     item.Metadata.WIPBranch,
		KnownGood:     item.Metadata.KnownGood,
		FixupLast:     item.FixupLast,
	}
}

func (e pinTableEntry) StatusCell() string {
	if e.Section == pinSectionBlocked {
		if e.FixupAttempts > 0 {
			return fmt.Sprintf("b%d", e.FixupAttempts)
		}
		return "blk"
	}
	if e.Checked {
		return "[x]"
	}
	return "[ ]"
}

type pinReloadMsg struct {
	queueItems   []pin.QueueItem
	blockedItems []pin.BlockedItem
	queueStamp   fileStamp
	blockedCount int
	err          error
	stampErr     error
	resetScroll  bool
}

type pinQueueEditDoneMsg struct {
	err error
}

type pinView struct {
	files                 pin.Files
	items                 []pinTableEntry
	allItems              []pinTableEntry
	queueAll              []pinTableEntry
	blockedAll            []pinTableEntry
	blockedCount          int
	section               pinSection
	table                 table.Model
	tableStyles           table.Styles
	detail                viewport.Model
	status                string
	err                   string
	mode                  pinMode
	focus                 pinFocus
	loading               bool
	reloadAgain           bool
	blockForm             *huh.Form
	blockReason           string
	moveForm              *huh.Form
	movePrepend           bool
	config                config.Config
	locations             paths.Locations
	logger                *tuiLogger
	clipboardWrite        func(string) error
	width                 int
	height                int
	queueStamp            fileStamp
	pendingSelectID       string
	validateAfterReload   bool
	queueSelectedID       string
	queueSelectedOffset   int
	blockedSelectedID     string
	blockedSelectedOffset int
}

const (
	defaultPinViewWidth = 80
)

func newPinView(cfg config.Config, locations paths.Locations) (*pinView, error) {
	files := pin.ResolveFiles(cfg.Paths.PinDir)
	view := &pinView{
		files:     files,
		mode:      pinModeTable,
		focus:     pinFocusTable,
		section:   pinSectionQueue,
		config:    cfg,
		locations: locations,
	}
	view.clipboardWrite = clipboard.WriteAll
	view.tableStyles = table.DefaultStyles()
	columns := pinTableColumns(defaultPinViewWidth, nil, view.tableStyles)
	view.table = table.New(table.WithColumns(columns), table.WithFocused(true), table.WithStyles(view.tableStyles))
	view.detail = viewport.New(80, 10)
	view.detail.Style = lipgloss.NewStyle()
	return view, nil
}

func (p *pinView) Update(msg tea.Msg, keys keyMap, loopMode loopMode) tea.Cmd {
	if pinCommandsBlocked(loopMode) && (p.mode == pinModeBlockForm || p.mode == pinModeMoveCheckedForm) {
		p.blockForm = nil
		p.blockReason = ""
		p.moveForm = nil
		p.mode = pinModeTable
		p.err = ""
		p.status = "Pin updates disabled while loop is running."
		return nil
	}
	if p.mode == pinModeBlockForm {
		model, cmd := p.blockForm.Update(msg)
		if form, ok := model.(*huh.Form); ok {
			p.blockForm = form
		}
		if p.blockForm.State == huh.StateCompleted {
			return p.finishBlock()
		} else if p.blockForm.State == huh.StateAborted {
			p.status = "Block cancelled"
			p.err = ""
			p.mode = pinModeTable
		}
		return cmd
	}
	if p.mode == pinModeMoveCheckedForm {
		if p.moveForm == nil {
			p.mode = pinModeTable
			p.status = "Move cancelled"
			p.err = ""
			return nil
		}
		model, cmd := p.moveForm.Update(msg)
		if form, ok := model.(*huh.Form); ok {
			p.moveForm = form
		}
		if p.moveForm.State == huh.StateCompleted {
			return p.finishMoveChecked()
		} else if p.moveForm.State == huh.StateAborted {
			p.status = "Move cancelled"
			p.err = ""
			p.mode = pinModeTable
		}
		return cmd
	}

	if reloadMsg, ok := msg.(pinReloadMsg); ok {
		prevSelectedID := p.selectedItemID()
		prevDetailOffset := p.detail.YOffset
		p.loading = false
		if reloadMsg.err != nil {
			p.err = reloadMsg.err.Error()
			p.status = ""
			p.reloadAgain = false
			p.validateAfterReload = false
			return nil
		}
		p.err = ""
		p.setItems(reloadMsg.queueItems, reloadMsg.blockedItems, prevSelectedID, prevDetailOffset, reloadMsg.resetScroll)
		p.blockedCount = reloadMsg.blockedCount
		if reloadMsg.stampErr == nil {
			p.queueStamp = reloadMsg.queueStamp
		}
		if reloadMsg.stampErr != nil {
			p.setRefreshError("Pin file watch error", reloadMsg.stampErr)
		}
		if p.reloadAgain {
			p.reloadAgain = false
			return p.reloadAsync(false)
		}
		if p.validateAfterReload {
			p.validateAfterReload = false
			p.runValidate()
		}
		return nil
	}
	if doneMsg, ok := msg.(pinQueueEditDoneMsg); ok {
		if doneMsg.err != nil {
			p.err = doneMsg.err.Error()
			p.status = ""
			return nil
		}
		p.err = ""
		p.status = ""
		p.validateAfterReload = true
		return p.reloadAsync(true)
	}
	if keyMsg, ok := msg.(tea.KeyMsg); ok {
		switch {
		case key.Matches(keyMsg, keys.TogglePane):
			if p.mode == pinModeTable {
				if p.focus == pinFocusTable {
					p.setFocus(pinFocusDetail)
				} else {
					p.setFocus(pinFocusTable)
				}
			}
			return nil
		case key.Matches(keyMsg, keys.ValidatePin):
			if p.commandsBlocked(loopMode) {
				return nil
			}
			p.runValidate()
			return nil
		case key.Matches(keyMsg, keys.EditQueue):
			if p.commandsBlocked(loopMode) {
				return nil
			}
			return p.editQueueCmd()
		case key.Matches(keyMsg, keys.TogglePinSection):
			if p.mode == pinModeTable {
				p.toggleSection(true)
			}
			return nil
		case key.Matches(keyMsg, keys.MoveChecked):
			if p.commandsBlocked(loopMode) {
				return nil
			}
			p.startMoveChecked()
			return nil
		case key.Matches(keyMsg, keys.BlockItem):
			if p.commandsBlocked(loopMode) {
				return nil
			}
			p.startBlock()
			return nil
		case key.Matches(keyMsg, keys.UnblockItemTop):
			if p.commandsBlocked(loopMode) {
				return nil
			}
			return p.requeueSelectedBlockedItem(true)
		case key.Matches(keyMsg, keys.UnblockItemBottom):
			if p.commandsBlocked(loopMode) {
				return nil
			}
			return p.requeueSelectedBlockedItem(false)
		case key.Matches(keyMsg, keys.CopyWIPBranch):
			if p.commandsBlocked(loopMode) {
				return nil
			}
			return p.copySelectedBlockedWIPBranch()
		case key.Matches(keyMsg, keys.CopyKnownGoodSHA):
			if p.commandsBlocked(loopMode) {
				return nil
			}
			return p.copySelectedBlockedKnownGood()
		case key.Matches(keyMsg, keys.ResetFixupMetadata):
			if p.commandsBlocked(loopMode) {
				return nil
			}
			return p.resetSelectedBlockedFixupMetadata()
		case key.Matches(keyMsg, keys.ToggleChecked):
			if p.commandsBlocked(loopMode) {
				return nil
			}
			return p.toggleChecked()
		}
	}

	if p.focus == pinFocusDetail {
		updated, cmd := p.detail.Update(msg)
		p.detail = updated
		return cmd
	}

	prevCursor := p.table.Cursor()
	updated, cmd := p.table.Update(msg)
	p.table = updated
	if p.table.Cursor() != prevCursor {
		p.syncDetail(true)
	}
	return cmd
}

func (p *pinView) IsTyping() bool {
	if p == nil {
		return false
	}
	return p.mode == pinModeBlockForm || p.mode == pinModeMoveCheckedForm
}

func (p *pinView) View() string {
	if p.mode == pinModeBlockForm && p.blockForm != nil {
		return withFinalNewline("Block item\n\n" + p.blockForm.View())
	}
	if p.mode == pinModeMoveCheckedForm && p.moveForm != nil {
		return withFinalNewline("Move checked items\n\n" + p.moveForm.View())
	}
	header := "Pin"
	status := p.statusLine()
	if status == "" {
		return withFinalNewline(header + "\n\n" + p.tableWithDetail())
	}
	return withFinalNewline(header + "\n" + status + "\n\n" + p.tableWithDetail())
}

func (p *pinView) Resize(width int, height int) {
	p.width = width
	p.height = height

	p.setTableColumns(width)
	header := "Pin"
	status := p.statusLine()
	blocks := []wrappedBlock{{Text: header, MinRows: 1}}
	if status == "" {
		blocks[0].BlankLinesAfter = 1
	} else {
		blocks = append(blocks, wrappedBlock{Text: status, MinRows: 1, BlankLinesAfter: 1})
	}
	chrome := chromeHeight(width, blocks...)
	available := remainingHeight(height, chrome)
	available = remainingHeight(available, 1)
	tableHeight, detailHeight := splitTwoPaneHeight(available, 2, 5)
	p.table.SetHeight(tableHeight)
	resizeViewportToFit(&p.detail, max(0, width), max(0, detailHeight), p.detail.Style)
	if p.mode == pinModeBlockForm && p.blockForm != nil {
		chrome := chromeHeight(
			width,
			wrappedBlock{Text: "Block item", MinRows: 1, BlankLinesAfter: 1},
		)
		formHeight := remainingHeight(height, chrome)
		p.blockForm = resizeHuhFormToFit(p.blockForm, width, formHeight)
	}
	if p.mode == pinModeMoveCheckedForm && p.moveForm != nil {
		chrome := chromeHeight(
			width,
			wrappedBlock{Text: "Move checked items", MinRows: 1, BlankLinesAfter: 1},
		)
		formHeight := remainingHeight(height, chrome)
		p.moveForm = resizeHuhFormToFit(p.moveForm, width, formHeight)
	}
}

func (p *pinView) statusLine() string {
	focusNote := p.focusLabel()
	if p.loading {
		return joinStatus("Loading pin...", focusNote)
	}
	if p.err != "" {
		return joinStatus("Error: "+p.err, focusNote)
	}
	if p.status != "" {
		return joinStatus(p.status, focusNote)
	}
	status := p.contextStatus()
	if status == "" {
		return focusNote
	}
	return joinStatus(status, focusNote)
}

func (p *pinView) tableWithDetail() string {
	left := p.table.View()
	detail := p.detail.View()
	return left + "\n\n" + detail
}

func (p *pinView) selectedItem() *pinTableEntry {
	if len(p.items) == 0 {
		return nil
	}
	idx := p.table.Cursor()
	if idx < 0 || idx >= len(p.items) {
		return nil
	}
	return &p.items[idx]
}

func (p *pinView) selectedItemID() string {
	item := p.selectedItem()
	if item == nil {
		return ""
	}
	return item.ID
}

func (p *pinView) restoreSelection(selectedID string) {
	if selectedID == "" {
		return
	}
	for idx, item := range p.items {
		if item.ID == selectedID {
			p.table.SetCursor(idx)
			return
		}
	}
}

func (p *pinView) clampCursor() {
	count := len(p.items)
	cursor := p.table.Cursor()
	if count <= 0 {
		p.table.SetCursor(0)
		return
	}
	if cursor < 0 {
		p.table.SetCursor(0)
		return
	}
	if cursor >= count {
		p.table.SetCursor(count - 1)
		return
	}
}

func (p *pinView) setRefreshError(prefix string, err error) {
	if err == nil {
		return
	}
	p.err = fmt.Sprintf("%s: %s", prefix, err.Error())
	p.status = ""
	if p.logger != nil {
		p.logger.Error(prefix, map[string]any{"error": err.Error()})
	}
}

func (p *pinView) reload() error {
	items, blockedCount, err := pin.ReadQueueSummary(p.files.QueuePath)
	if err != nil {
		return err
	}
	blockedItems, err := pin.ReadBlockedItems(p.files.QueuePath)
	if err != nil {
		return err
	}
	p.setItems(items, blockedItems, p.selectedItemID(), p.detail.YOffset, true)
	p.blockedCount = blockedCount
	if stamp, err := getFileStamp(p.files.QueuePath); err == nil {
		p.queueStamp = stamp
	} else {
		p.setRefreshError("Pin file watch error", err)
	}
	return nil
}

func (p *pinView) reloadAsync(resetScroll bool) tea.Cmd {
	if p.loading {
		p.reloadAgain = true
		return nil
	}
	p.loading = true
	p.reloadAgain = false
	p.status = ""
	p.err = ""
	files := p.files
	return func() tea.Msg {
		items, blockedCount, err := pin.ReadQueueSummary(files.QueuePath)
		if err != nil {
			return pinReloadMsg{err: err, resetScroll: resetScroll}
		}
		blockedItems, err := pin.ReadBlockedItems(files.QueuePath)
		if err != nil {
			return pinReloadMsg{err: err, resetScroll: resetScroll}
		}
		var stamp fileStamp
		var stampErr error
		if current, err := getFileStamp(files.QueuePath); err == nil {
			stamp = current
		} else {
			stampErr = err
		}
		return pinReloadMsg{
			queueItems:   items,
			blockedItems: blockedItems,
			queueStamp:   stamp,
			blockedCount: blockedCount,
			stampErr:     stampErr,
			resetScroll:  resetScroll,
		}
	}
}

func (p *pinView) runValidate() {
	if err := pin.ValidatePin(p.files, p.config.ProjectType); err != nil {
		p.err = err.Error()
		p.status = ""
		return
	}
	p.err = ""
	p.status = ">> [RALPH] Pin validation OK."
}

func (p *pinView) editQueueCmd() tea.Cmd {
	editor := strings.TrimSpace(os.Getenv("EDITOR"))
	cmd, err := buildEditorCommand(editor, p.files.QueuePath)
	if err != nil {
		p.err = err.Error()
		p.status = ""
		return nil
	}
	p.err = ""
	p.status = "Opening editor..."
	return tea.ExecProcess(cmd, func(err error) tea.Msg {
		return pinQueueEditDoneMsg{err: err}
	})
}

func (p *pinView) startMoveChecked() {
	if p.section != pinSectionQueue {
		p.status = "Switch to Queue to move checked items."
		p.err = ""
		return
	}
	if p.queueCheckedCount(p.queueAll) == 0 {
		p.status = "No checked items to move."
		p.err = ""
		return
	}
	p.movePrepend = true
	p.moveForm = huh.NewForm(
		huh.NewGroup(
			huh.NewConfirm().
				Title("Prepend moved items to Done? (recommended)").
				Value(&p.movePrepend),
		),
	).WithShowHelp(false)
	p.mode = pinModeMoveCheckedForm
	p.status = ""
	p.err = ""
	p.Resize(p.width, p.height)
}

func (p *pinView) finishMoveChecked() tea.Cmd {
	p.mode = pinModeTable
	p.moveForm = nil
	ids, err := pin.MoveCheckedToDone(p.files.QueuePath, p.files.DonePath, pin.DoneWriteOptions{
		Prepend:        p.movePrepend,
		RetentionLimit: p.config.Paths.DoneRetentionLimit,
	})
	if err != nil {
		p.err = err.Error()
		p.status = ""
		return nil
	}
	p.err = ""
	summary := pin.SummarizeIDs(ids)
	if summary == "" {
		p.status = "No checked items moved."
	} else {
		p.status = fmt.Sprintf("Moved: %s", summary)
	}
	return p.reloadAsync(true)
}

func (p *pinView) startBlock() {
	item := p.selectedItem()
	if item == nil || item.ID == "" || item.Section != pinSectionQueue {
		p.err = "No queue item selected."
		p.status = ""
		return
	}
	p.blockReason = ""
	p.blockForm = huh.NewForm(
		huh.NewGroup(
			huh.NewText().
				Title(fmt.Sprintf("Block %s: reason lines", item.ID)).
				Value(&p.blockReason).
				Validate(requireNonEmpty("blocked reason")),
		),
	).WithShowHelp(false)
	p.mode = pinModeBlockForm
	p.status = ""
	p.err = ""
	p.Resize(p.width, p.height)
}

func (p *pinView) finishBlock() tea.Cmd {
	item := p.selectedItem()
	if item == nil || item.Section != pinSectionQueue {
		p.err = "No queue item selected."
		p.status = ""
		p.mode = pinModeTable
		return nil
	}
	reasonLines := make([]string, 0)
	for _, line := range strings.Split(p.blockReason, "\n") {
		if strings.TrimSpace(line) != "" {
			reasonLines = append(reasonLines, line)
		}
	}
	if len(reasonLines) == 0 {
		p.err = "At least one reason line is required."
		p.status = ""
		p.mode = pinModeTable
		return nil
	}
	ok, err := pin.BlockItem(p.files.QueuePath, item.ID, reasonLines, pin.Metadata{})
	if err != nil {
		p.err = err.Error()
		p.status = ""
		p.mode = pinModeTable
		return nil
	}
	if !ok {
		p.err = fmt.Sprintf("Item %s not found in Queue.", item.ID)
		p.status = ""
		p.mode = pinModeTable
		return nil
	}
	p.status = fmt.Sprintf("Blocked %s", item.ID)
	p.err = ""
	p.mode = pinModeTable
	return p.reloadAsync(true)
}

func (p *pinView) toggleChecked() tea.Cmd {
	item := p.selectedItem()
	if item == nil || item.ID == "" || item.Section != pinSectionQueue {
		p.err = "No queue item selected."
		p.status = ""
		return nil
	}
	ok, checked, err := pin.ToggleQueueItemChecked(p.files.QueuePath, item.ID)
	if err != nil {
		p.err = err.Error()
		p.status = ""
		return nil
	}
	if !ok {
		p.err = fmt.Sprintf("Item %s not found in Queue.", item.ID)
		p.status = ""
		return nil
	}
	state := "unchecked"
	if checked {
		state = "checked"
	}
	p.status = fmt.Sprintf("Marked %s as %s", item.ID, state)
	p.err = ""
	return p.reloadAsync(false)
}

func (p *pinView) selectedBlockedItem(action string) *pinTableEntry {
	item := p.selectedItem()
	if item == nil || item.ID == "" || item.Section != pinSectionBlocked {
		p.status = fmt.Sprintf("Switch to Blocked to %s.", action)
		p.err = ""
		return nil
	}
	return item
}

func (p *pinView) requeueSelectedBlockedItem(insertAtTop bool) tea.Cmd {
	item := p.selectedBlockedItem("requeue blocked items")
	if item == nil {
		return nil
	}
	ok, err := pin.RequeueBlockedItem(p.files.QueuePath, item.ID, pin.RequeueOptions{InsertAtTop: insertAtTop})
	if err != nil {
		p.err = err.Error()
		p.status = ""
		return nil
	}
	if !ok {
		p.err = fmt.Sprintf("Item %s not found in Blocked.", item.ID)
		p.status = ""
		return nil
	}
	if insertAtTop {
		p.status = fmt.Sprintf("Requeued %s to top of Queue.", item.ID)
	} else {
		p.status = fmt.Sprintf("Requeued %s to bottom of Queue.", item.ID)
	}
	p.err = ""
	p.pendingSelectID = item.ID
	p.section = pinSectionQueue
	return p.reloadAsync(true)
}

func (p *pinView) copySelectedBlockedMetadata(label string, value string, itemID string) tea.Cmd {
	value = strings.TrimSpace(value)
	if value == "" {
		p.status = fmt.Sprintf("No %s metadata for %s.", label, itemID)
		p.err = ""
		return nil
	}
	if p.clipboardWrite == nil {
		p.err = "Clipboard unavailable."
		p.status = ""
		return nil
	}
	if err := p.clipboardWrite(value); err != nil {
		p.err = err.Error()
		p.status = ""
		return nil
	}
	p.status = fmt.Sprintf("Copied %s for %s: %s", label, itemID, value)
	p.err = ""
	return nil
}

func (p *pinView) copySelectedBlockedWIPBranch() tea.Cmd {
	item := p.selectedBlockedItem("copy WIP branch")
	if item == nil {
		return nil
	}
	return p.copySelectedBlockedMetadata("WIP branch", item.WIPBranch, item.ID)
}

func (p *pinView) copySelectedBlockedKnownGood() tea.Cmd {
	item := p.selectedBlockedItem("copy known-good SHA")
	if item == nil {
		return nil
	}
	return p.copySelectedBlockedMetadata("known-good SHA", item.KnownGood, item.ID)
}

func (p *pinView) resetSelectedBlockedFixupMetadata() tea.Cmd {
	item := p.selectedBlockedItem("reset fixup metadata")
	if item == nil {
		return nil
	}
	found, changed, err := pin.ResetFixupMetadata(p.files.QueuePath, item.ID)
	if err != nil {
		p.err = err.Error()
		p.status = ""
		return nil
	}
	if !found {
		p.err = fmt.Sprintf("Item %s not found in Blocked.", item.ID)
		p.status = ""
		return nil
	}
	if !changed {
		p.status = fmt.Sprintf("No fixup metadata to reset for %s.", item.ID)
		p.err = ""
		return nil
	}
	p.status = fmt.Sprintf("Reset fixup metadata for %s.", item.ID)
	p.err = ""
	return p.reloadAsync(false)
}

func (p *pinView) commandsBlocked(loopMode loopMode) bool {
	if !pinCommandsBlocked(loopMode) {
		return false
	}
	p.err = ""
	p.status = "Pin updates disabled while loop is running."
	return true
}

func (p *pinView) syncDetail(resetScroll bool) {
	item := p.selectedItem()
	content := "No item selected."
	if item != nil {
		content = strings.Join(item.Lines, "\n")
	}
	p.detail.SetContent(content)
	if resetScroll {
		p.detail.GotoTop()
	}
}

func (p *pinView) syncDetailWithOffset(resetScroll bool, offset int) {
	item := p.selectedItem()
	content := "No item selected."
	if item != nil {
		content = strings.Join(item.Lines, "\n")
	}
	p.detail.SetContent(content)
	if resetScroll {
		p.detail.GotoTop()
		return
	}
	p.detail.SetYOffset(offset)
}

func (p *pinView) SetConfig(cfg config.Config, locations paths.Locations) error {
	p.config = cfg
	p.locations = locations
	p.files = pin.ResolveFiles(cfg.Paths.PinDir)
	return p.reload()
}

func (p *pinView) RefreshIfNeeded() tea.Cmd {
	if p.mode != pinModeTable {
		return nil
	}
	stamp, changed, err := fileChanged(p.files.QueuePath, p.queueStamp)
	if err != nil {
		p.setRefreshError("Pin file watch error", err)
		return nil
	}
	if changed {
		p.queueStamp = stamp
		return p.reloadAsync(false)
	}
	return nil
}

func (p *pinView) Focus() {
	p.setFocus(p.focus)
}

func (p *pinView) Blur() {
	p.table.Blur()
}

func (p *pinView) setFocus(focus pinFocus) {
	p.focus = focus
	if focus == pinFocusTable {
		p.table.Focus()
	} else {
		p.table.Blur()
	}
}

func (p *pinView) focusLabel() string {
	if p.mode != pinModeTable {
		return ""
	}
	if p.focus == pinFocusDetail {
		return "Focus: detail (ctrl+t)"
	}
	return "Focus: table (ctrl+t)"
}

func (p *pinView) cacheSelection() {
	selectedID := p.selectedItemID()
	offset := p.detail.YOffset
	if p.section == pinSectionBlocked {
		p.blockedSelectedID = selectedID
		p.blockedSelectedOffset = offset
		return
	}
	p.queueSelectedID = selectedID
	p.queueSelectedOffset = offset
}

func (p *pinView) toggleSection(resetScroll bool) {
	p.cacheSelection()
	if p.section == pinSectionBlocked {
		p.section = pinSectionQueue
		p.applyActiveItems(p.queueSelectedID, p.queueSelectedOffset, resetScroll)
	} else {
		p.section = pinSectionBlocked
		p.applyActiveItems(p.blockedSelectedID, p.blockedSelectedOffset, resetScroll)
	}
	p.status = ""
	p.err = ""
}

func (p *pinView) contextStatus() string {
	if p.mode != pinModeTable {
		return ""
	}
	parts := []string{
		fmt.Sprintf("View: %s", p.section.Label()),
		fmt.Sprintf("Queue: %d (checked %d)", len(p.queueAll), p.queueCheckedCount(p.queueAll)),
		fmt.Sprintf("Blocked: %d", p.blockedItemsCount()),
	}
	return strings.Join(parts, " | ")
}

func (p *pinView) blockedItemsCount() int {
	if p.blockedCount > 0 {
		return p.blockedCount
	}
	return len(p.blockedAll)
}

func (p *pinView) queueCheckedCount(items []pinTableEntry) int {
	count := 0
	for _, item := range items {
		if item.Checked {
			count++
		}
	}
	return count
}

func joinStatus(primary string, secondary string) string {
	if primary == "" {
		return secondary
	}
	if secondary == "" {
		return primary
	}
	return primary + " | " + secondary
}

func trimTitle(header string) string {
	return pin.TrimCheckboxPrefix(header)
}

func (p *pinView) setTableColumns(width int) {
	columns := pinTableColumns(width, p.items, p.tableStyles)
	p.table.SetColumns(columns)
}

func pinTableColumns(width int, items []pinTableEntry, styles table.Styles) []table.Column {
	statusWidth := max(lipgloss.Width("Status"), lipgloss.Width("[x]"))
	idWidth := lipgloss.Width("ID")
	for _, item := range items {
		statusWidth = max(statusWidth, lipgloss.Width(item.StatusCell()))
		idWidth = max(idWidth, lipgloss.Width(item.ID))
	}
	minTitleWidth := lipgloss.Width("Title")
	cellFrameW, _ := styles.Cell.GetFrameSize()
	headerFrameW, _ := styles.Header.GetFrameSize()
	frameW := max(cellFrameW, headerFrameW)
	titleWidth := minTitleWidth
	if width > 0 {
		available := width - frameW*3 - statusWidth - idWidth
		if available < minTitleWidth {
			titleWidth = max(0, available)
		} else {
			titleWidth = available
		}
	}
	return []table.Column{
		{Title: "Status", Width: statusWidth},
		{Title: "ID", Width: idWidth},
		{Title: "Title", Width: titleWidth},
	}
}

func requireNonEmpty(label string) func(string) error {
	return func(value string) error {
		if strings.TrimSpace(value) == "" {
			return fmt.Errorf("%s must be set", label)
		}
		return nil
	}
}

func (p *pinView) setItems(
	queueItems []pin.QueueItem,
	blockedItems []pin.BlockedItem,
	prevSelectedID string,
	prevDetailOffset int,
	resetScroll bool,
) {
	p.queueAll = make([]pinTableEntry, 0, len(queueItems))
	for _, item := range queueItems {
		p.queueAll = append(p.queueAll, queueEntry(item))
	}
	p.blockedAll = make([]pinTableEntry, 0, len(blockedItems))
	for _, item := range blockedItems {
		p.blockedAll = append(p.blockedAll, blockedEntry(item))
	}
	p.applyActiveItems(prevSelectedID, prevDetailOffset, resetScroll)
}

func (p *pinView) applyActiveItems(prevSelectedID string, prevDetailOffset int, resetScroll bool) {
	if p.section == pinSectionBlocked {
		p.allItems = p.blockedAll
	} else {
		p.allItems = p.queueAll
	}
	p.items = p.allItems
	p.table.SetRows(makePinRows(p.items))
	p.setTableColumns(p.width)
	if prevSelectedID != "" {
		p.restoreSelection(prevSelectedID)
	}
	if p.pendingSelectID != "" && p.section == pinSectionQueue {
		p.restoreSelection(p.pendingSelectID)
		if p.selectedItemID() == p.pendingSelectID {
			p.pendingSelectID = ""
		}
	}
	p.clampCursor()
	sameSelection := prevSelectedID != "" && prevSelectedID == p.selectedItemID()
	if sameSelection && !resetScroll {
		p.syncDetailWithOffset(false, prevDetailOffset)
	} else {
		p.syncDetail(resetScroll || !sameSelection)
	}
	p.cacheSelection()
}

func (p *pinView) SearchEntries() []pinTableEntry {
	if p == nil {
		return nil
	}
	items := make([]pinTableEntry, 0, len(p.queueAll)+len(p.blockedAll))
	items = append(items, p.queueAll...)
	items = append(items, p.blockedAll...)
	return items
}

func (p *pinView) SelectItem(section pinSection, itemID string) bool {
	return p.selectItemInSection(section, itemID, false)
}

func (p *pinView) SelectItemByID(itemID string) bool {
	return p.selectItemInSection(pinSectionQueue, itemID, true)
}

func (p *pinView) selectItemInSection(section pinSection, itemID string, allowPending bool) bool {
	if p == nil {
		return false
	}
	itemID = strings.TrimSpace(itemID)
	if itemID == "" {
		return false
	}
	if allowPending && section == pinSectionQueue {
		p.pendingSelectID = itemID
	}
	if p.section != section {
		p.cacheSelection()
		p.section = section
	}
	p.applyActiveItems(itemID, 0, true)
	if p.selectedItemID() == itemID {
		if allowPending && section == pinSectionQueue {
			p.pendingSelectID = ""
		}
		return true
	}
	return false
}

func makePinRows(items []pinTableEntry) []table.Row {
	rows := make([]table.Row, 0, len(items))
	for _, item := range items {
		rows = append(rows, table.Row{item.StatusCell(), item.ID, trimTitle(item.Header)})
	}
	return rows
}

func pinCommandsBlocked(loopMode loopMode) bool {
	return loopMode == loopRunning || loopMode == loopStopping
}

func buildEditorCommand(editor string, filePath string) (*exec.Cmd, error) {
	if strings.TrimSpace(editor) == "" {
		editor = "vi"
	}
	args, err := shlex.Split(editor)
	if err != nil {
		return nil, fmt.Errorf("parse $EDITOR: %w", err)
	}
	if len(args) == 0 {
		return nil, fmt.Errorf("editor command is empty")
	}
	args = append(args, filePath)
	cmd := exec.Command(args[0], args[1:]...)
	cmd.Env = os.Environ()
	return cmd, nil
}
