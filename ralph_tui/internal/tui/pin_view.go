// Package tui provides the pin queue screen and operations.
// Entrypoint: pinView.
package tui

import (
	"fmt"
	"regexp"
	"strings"

	"github.com/charmbracelet/bubbles/key"
	"github.com/charmbracelet/bubbles/table"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/huh"
	"github.com/charmbracelet/lipgloss"
	"github.com/mitchfultz/ralph/ralph_tui/internal/config"
	"github.com/mitchfultz/ralph/ralph_tui/internal/paths"
	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
)

type pinMode int

const (
	pinModeTable pinMode = iota
	pinModeBlockForm
)

type pinFocus int

const (
	pinFocusTable pinFocus = iota
	pinFocusDetail
)

type pinReloadMsg struct {
	items       []pin.QueueItem
	queueStamp  fileStamp
	blocked     int
	err         error
	stampErr    error
	resetScroll bool
}

type pinView struct {
	files           pin.Files
	items           []pin.QueueItem
	allItems        []pin.QueueItem
	blockedCount    int
	table           table.Model
	tableStyles     table.Styles
	detail          viewport.Model
	status          string
	err             string
	mode            pinMode
	focus           pinFocus
	loading         bool
	reloadAgain     bool
	blockForm       *huh.Form
	blockReason     string
	config          config.Config
	locations       paths.Locations
	logger          *tuiLogger
	width           int
	height          int
	queueStamp      fileStamp
	searchTerm      string
	searchAnchor    string
	searchOffset    int
	pendingSelectID string
}

const (
	defaultPinViewWidth = 80
	pinViewChromeHeight = 4
)

var pinTagPattern = regexp.MustCompile(`\[(db|ui|code|ops|docs)\]`)

func newPinView(cfg config.Config, locations paths.Locations) (*pinView, error) {
	files := pin.ResolveFiles(cfg.Paths.PinDir)
	view := &pinView{
		files:     files,
		mode:      pinModeTable,
		focus:     pinFocusTable,
		config:    cfg,
		locations: locations,
	}
	view.tableStyles = table.DefaultStyles()
	columns := pinTableColumns(defaultPinViewWidth, nil, view.tableStyles)
	view.table = table.New(table.WithColumns(columns), table.WithFocused(true), table.WithStyles(view.tableStyles))
	view.detail = viewport.New(80, 10)
	view.detail.Style = lipgloss.NewStyle()
	return view, nil
}

func (p *pinView) Update(msg tea.Msg, keys keyMap) tea.Cmd {
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

	if reloadMsg, ok := msg.(pinReloadMsg); ok {
		prevSelectedID := p.selectedItemID()
		prevDetailOffset := p.detail.YOffset
		p.loading = false
		if reloadMsg.err != nil {
			p.err = reloadMsg.err.Error()
			p.status = ""
			p.reloadAgain = false
			return nil
		}
		p.err = ""
		p.setQueueItems(reloadMsg.items, prevSelectedID, prevDetailOffset, reloadMsg.resetScroll)
		p.blockedCount = reloadMsg.blocked
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
		return nil
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
			p.runValidate()
			return nil
		case key.Matches(keyMsg, keys.MoveChecked):
			return p.runMoveChecked()
		case key.Matches(keyMsg, keys.BlockItem):
			p.startBlock()
			return nil
		case key.Matches(keyMsg, keys.ToggleChecked):
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

func (p *pinView) HandlesTabNavigation() bool {
	return p.mode == pinModeBlockForm && p.blockForm != nil
}

func (p *pinView) View() string {
	if p.mode == pinModeBlockForm && p.blockForm != nil {
		return withFinalNewline("Block item\n\n" + p.blockForm.View())
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
	available := height - pinViewChromeHeight
	if available < 0 {
		available = 0
	}
	tableHeight := 0
	detailHeight := 0
	if available > 0 {
		tableHeight = available * 2 / 5
		if tableHeight < 1 {
			tableHeight = 1
		}
		if tableHeight > available {
			tableHeight = available
		}
		detailHeight = available - tableHeight
		if detailHeight < 1 && available > 1 {
			detailHeight = 1
			tableHeight = available - 1
		}
	}
	p.table.SetHeight(tableHeight)
	resizeViewportToFit(&p.detail, max(0, width), max(0, detailHeight), p.detail.Style)
	if p.mode == pinModeBlockForm && p.blockForm != nil {
		formHeight := height - 2
		if formHeight < 1 {
			formHeight = 1
		}
		p.blockForm = p.blockForm.WithWidth(max(1, width))
		p.blockForm = p.blockForm.WithHeight(max(1, formHeight))
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
	if p.searchTerm != "" {
		return joinStatus(fmt.Sprintf("Filter: %s", p.searchTerm), focusNote)
	}
	return focusNote
}

func (p *pinView) tableWithDetail() string {
	left := p.table.View()
	detail := p.detail.View()
	return left + "\n\n" + detail
}

func (p *pinView) selectedItem() *pin.QueueItem {
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
	items, blocked, err := pin.ReadQueueSummary(p.files.QueuePath)
	if err != nil {
		return err
	}
	p.setQueueItems(items, p.selectedItemID(), p.detail.YOffset, true)
	p.blockedCount = blocked
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
		items, blocked, err := pin.ReadQueueSummary(files.QueuePath)
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
			items:       items,
			queueStamp:  stamp,
			blocked:     blocked,
			stampErr:    stampErr,
			resetScroll: resetScroll,
		}
	}
}

func (p *pinView) runValidate() {
	if err := pin.ValidatePin(p.files); err != nil {
		p.err = err.Error()
		p.status = ""
		return
	}
	p.err = ""
	p.status = ">> [RALPH] Pin validation OK."
}

func (p *pinView) runMoveChecked() tea.Cmd {
	ids, err := pin.MoveCheckedToDone(p.files.QueuePath, p.files.DonePath, false)
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
	if item == nil || item.ID == "" {
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
	if item == nil {
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
	if item == nil || item.ID == "" {
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
	trimmed := strings.TrimSpace(strings.TrimPrefix(strings.TrimPrefix(header, "- [ ]"), "- [x]"))
	return trimmed
}

func (p *pinView) setTableColumns(width int) {
	columns := pinTableColumns(width, p.items, p.tableStyles)
	p.table.SetColumns(columns)
}

func pinTableColumns(width int, items []pin.QueueItem, styles table.Styles) []table.Column {
	statusWidth := max(lipgloss.Width("Status"), lipgloss.Width("[x]"))
	idWidth := lipgloss.Width("ID")
	for _, item := range items {
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

func (p *pinView) setQueueItems(items []pin.QueueItem, prevSelectedID string, prevDetailOffset int, resetScroll bool) {
	p.allItems = items
	displayItems := items
	if p.searchTerm != "" {
		displayItems = filterQueueItems(items, p.searchTerm)
	}
	p.items = displayItems
	p.table.SetRows(makePinRows(displayItems))
	p.setTableColumns(p.width)
	if prevSelectedID != "" {
		p.restoreSelection(prevSelectedID)
	}
	if p.pendingSelectID != "" {
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
}

func (p *pinView) SelectItemByID(itemID string) bool {
	if p == nil {
		return false
	}
	itemID = strings.TrimSpace(itemID)
	if itemID == "" {
		return false
	}
	p.pendingSelectID = itemID
	if p.searchTerm != "" {
		p.searchTerm = ""
		p.items = p.allItems
		p.table.SetRows(makePinRows(p.items))
		p.setTableColumns(p.width)
	}
	p.restoreSelection(itemID)
	p.clampCursor()
	if p.selectedItemID() == itemID {
		p.pendingSelectID = ""
		p.syncDetail(true)
		return true
	}
	return false
}

func (p *pinView) ApplySearch(term string) error {
	term = strings.TrimSpace(term)
	if term == "" && p.searchTerm == "" {
		return nil
	}
	prevSelectedID := p.selectedItemID()
	prevDetailOffset := p.detail.YOffset
	if p.searchTerm == "" && term != "" {
		p.searchAnchor = prevSelectedID
		p.searchOffset = prevDetailOffset
	}
	p.searchTerm = term
	p.items = filterQueueItems(p.allItems, term)
	p.table.SetRows(makePinRows(p.items))
	p.setTableColumns(p.width)
	if term == "" {
		if p.searchAnchor != "" {
			p.restoreSelection(p.searchAnchor)
		} else if prevSelectedID != "" {
			p.restoreSelection(prevSelectedID)
		}
		p.clampCursor()
		if p.searchAnchor != "" && p.searchAnchor == p.selectedItemID() {
			p.syncDetailWithOffset(false, p.searchOffset)
		} else {
			p.syncDetail(true)
		}
		p.searchAnchor = ""
		p.searchOffset = 0
		return nil
	}
	if prevSelectedID != "" {
		p.restoreSelection(prevSelectedID)
	}
	p.clampCursor()
	p.syncDetail(true)
	return nil
}

func (p *pinView) CancelSearch() {
	if p.searchTerm == "" {
		return
	}
	p.searchTerm = ""
	p.items = p.allItems
	p.table.SetRows(makePinRows(p.items))
	p.setTableColumns(p.width)
	if p.searchAnchor != "" {
		p.restoreSelection(p.searchAnchor)
	}
	p.clampCursor()
	if p.searchAnchor != "" && p.searchAnchor == p.selectedItemID() {
		p.syncDetailWithOffset(false, p.searchOffset)
	} else {
		p.syncDetail(true)
	}
	p.searchAnchor = ""
	p.searchOffset = 0
}

func (p *pinView) FinalizeSearch() {
	if p.searchTerm == "" {
		p.CancelSearch()
		return
	}
	p.searchAnchor = ""
	p.searchOffset = 0
}

func makePinRows(items []pin.QueueItem) []table.Row {
	rows := make([]table.Row, 0, len(items))
	for _, item := range items {
		status := "[ ]"
		if item.Checked {
			status = "[x]"
		}
		rows = append(rows, table.Row{status, item.ID, trimTitle(item.Header)})
	}
	return rows
}

func filterQueueItems(items []pin.QueueItem, term string) []pin.QueueItem {
	term = strings.ToLower(strings.TrimSpace(term))
	if term == "" {
		return items
	}
	parts := strings.Fields(term)
	filtered := make([]pin.QueueItem, 0, len(items))
	for _, item := range items {
		if matchesQueueItem(item, parts) {
			filtered = append(filtered, item)
		}
	}
	return filtered
}

func matchesQueueItem(item pin.QueueItem, parts []string) bool {
	if len(parts) == 0 {
		return true
	}
	tags := strings.Join(extractPinTags(item.Header), " ")
	haystack := strings.ToLower(strings.Join([]string{item.ID, trimTitle(item.Header), tags}, " "))
	for _, part := range parts {
		if !strings.Contains(haystack, part) {
			return false
		}
	}
	return true
}

func extractPinTags(header string) []string {
	matches := pinTagPattern.FindAllStringSubmatch(header, -1)
	if len(matches) == 0 {
		return nil
	}
	tags := make([]string, 0, len(matches))
	for _, match := range matches {
		if len(match) < 2 {
			continue
		}
		tags = append(tags, match[1])
	}
	return tags
}
