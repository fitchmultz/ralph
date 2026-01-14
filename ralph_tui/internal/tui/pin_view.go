// Package tui provides the pin queue screen and operations.
// Entrypoint: pinView.
package tui

import (
	"fmt"
	"strings"
	"time"

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
	rows        []table.Row
	modTime     time.Time
	err         error
	resetScroll bool
}

type pinView struct {
	files       pin.Files
	items       []pin.QueueItem
	table       table.Model
	detail      viewport.Model
	status      string
	err         string
	mode        pinMode
	focus       pinFocus
	loading     bool
	reloadAgain bool
	blockForm   *huh.Form
	blockReason string
	config      config.Config
	locations   paths.Locations
	width       int
	height      int
	queueMTime  time.Time
}

func newPinView(cfg config.Config, locations paths.Locations) (*pinView, error) {
	files := pin.ResolveFiles(cfg.Paths.PinDir)
	view := &pinView{
		files:     files,
		mode:      pinModeTable,
		focus:     pinFocusTable,
		config:    cfg,
		locations: locations,
	}
	columns := []table.Column{
		{Title: "Status", Width: 6},
		{Title: "ID", Width: 10},
		{Title: "Title", Width: 60},
	}
	view.table = table.New(table.WithColumns(columns), table.WithFocused(true))
	view.detail = viewport.New(80, 10)
	view.detail.Style = lipgloss.NewStyle().Padding(0, 1)
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
		p.loading = false
		if reloadMsg.err != nil {
			p.err = reloadMsg.err.Error()
			p.status = ""
			return nil
		}
		p.err = ""
		p.items = reloadMsg.items
		p.table.SetRows(reloadMsg.rows)
		if !reloadMsg.modTime.IsZero() {
			p.queueMTime = reloadMsg.modTime
		}
		p.syncDetail(reloadMsg.resetScroll)
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

func (p *pinView) View() string {
	if p.mode == pinModeBlockForm && p.blockForm != nil {
		return withFinalNewline("Block item\n\n" + p.blockForm.View())
	}
	status := p.statusLine()
	return withFinalNewline(status + "\n\n" + p.tableWithDetail())
}

func (p *pinView) Resize(width int, height int) {
	p.width = width
	p.height = height

	statusWidth := 6
	idWidth := 10
	titleWidth := width - statusWidth - idWidth - 6
	if titleWidth < 0 {
		titleWidth = 0
	}
	if width >= statusWidth+idWidth+6+20 && titleWidth < 20 {
		titleWidth = 20
	}
	p.table.SetColumns([]table.Column{
		{Title: "Status", Width: statusWidth},
		{Title: "ID", Width: idWidth},
		{Title: "Title", Width: titleWidth},
	})
	available := height - 3
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

func (p *pinView) reload() error {
	items, err := pin.ReadQueueItems(p.files.QueuePath)
	if err != nil {
		return err
	}
	p.items = items
	rows := make([]table.Row, 0, len(items))
	for _, item := range items {
		status := "[ ]"
		if item.Checked {
			status = "[x]"
		}
		rows = append(rows, table.Row{status, item.ID, trimTitle(item.Header)})
	}
	p.table.SetRows(rows)
	if modTime, err := fileModTime(p.files.QueuePath); err == nil {
		p.queueMTime = modTime
	}
	p.syncDetail(true)
	return nil
}

func (p *pinView) reloadAsync(resetScroll bool) tea.Cmd {
	if p.loading {
		p.reloadAgain = true
		return nil
	}
	p.loading = true
	p.status = ""
	p.err = ""
	files := p.files
	return func() tea.Msg {
		items, err := pin.ReadQueueItems(files.QueuePath)
		if err != nil {
			return pinReloadMsg{err: err, resetScroll: resetScroll}
		}
		rows := make([]table.Row, 0, len(items))
		for _, item := range items {
			status := "[ ]"
			if item.Checked {
				status = "[x]"
			}
			rows = append(rows, table.Row{status, item.ID, trimTitle(item.Header)})
		}
		var modTime time.Time
		if mod, err := fileModTime(files.QueuePath); err == nil {
			modTime = mod
		}
		return pinReloadMsg{items: items, rows: rows, modTime: modTime, resetScroll: resetScroll}
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
	modTime, changed, err := fileChanged(p.files.QueuePath, p.queueMTime)
	if err != nil {
		return nil
	}
	if changed {
		p.queueMTime = modTime
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

func requireNonEmpty(label string) func(string) error {
	return func(value string) error {
		if strings.TrimSpace(value) == "" {
			return fmt.Errorf("%s must be set", label)
		}
		return nil
	}
}
