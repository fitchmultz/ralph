// Package tui provides the pin queue screen and operations.
// Entrypoint: pinView.
package tui

import (
	"fmt"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/key"
	"github.com/charmbracelet/bubbles/table"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/huh"
	"github.com/charmbracelet/lipgloss"
	"github.com/mitchfultz/idaho-fraud/ralph_tui/internal/config"
	"github.com/mitchfultz/idaho-fraud/ralph_tui/internal/paths"
	"github.com/mitchfultz/idaho-fraud/ralph_tui/internal/pin"
)

type pinMode int

const (
	pinModeTable pinMode = iota
	pinModeBlockForm
)

type pinView struct {
	files       pin.Files
	items       []pin.QueueItem
	table       table.Model
	status      string
	err         string
	mode        pinMode
	blockForm   *huh.Form
	blockReason string
	config      config.Config
	locations   paths.Locations
	width       int
	height      int
	queueMTime  time.Time
}

func newPinView(cfg config.Config, locations paths.Locations) (*pinView, error) {
	files := pin.ResolveFiles(cfg.Paths.PinDir, locations.RepoRoot)
	view := &pinView{
		files:     files,
		mode:      pinModeTable,
		config:    cfg,
		locations: locations,
	}
	columns := []table.Column{
		{Title: "Status", Width: 6},
		{Title: "ID", Width: 10},
		{Title: "Title", Width: 60},
	}
	view.table = table.New(table.WithColumns(columns), table.WithFocused(true))
	if err := view.reload(); err != nil {
		return nil, err
	}
	return view, nil
}

func (p *pinView) Update(msg tea.Msg, keys keyMap) tea.Cmd {
	if p.mode == pinModeBlockForm {
		model, cmd := p.blockForm.Update(msg)
		if form, ok := model.(*huh.Form); ok {
			p.blockForm = form
		}
		if p.blockForm.State == huh.StateCompleted {
			p.finishBlock()
		} else if p.blockForm.State == huh.StateAborted {
			p.status = "Block cancelled"
			p.err = ""
			p.mode = pinModeTable
		}
		return cmd
	}

	if keyMsg, ok := msg.(tea.KeyMsg); ok {
		switch {
		case key.Matches(keyMsg, keys.ValidatePin):
			p.runValidate()
			return nil
		case key.Matches(keyMsg, keys.MoveChecked):
			p.runMoveChecked()
			return nil
		case key.Matches(keyMsg, keys.BlockItem):
			p.startBlock()
			return nil
		}
	}

	updated, cmd := p.table.Update(msg)
	p.table = updated
	return cmd
}

func (p *pinView) View() string {
	if p.mode == pinModeBlockForm && p.blockForm != nil {
		return strings.TrimSpace("Block item\n\n"+p.blockForm.View()) + "\n"
	}
	status := p.statusLine()
	return strings.TrimSpace(status+"\n\n"+p.tableWithDetail()) + "\n"
}

func (p *pinView) Resize(width int, height int) {
	if width <= 0 || height <= 0 {
		return
	}
	p.width = width
	p.height = height

	statusWidth := 6
	idWidth := 10
	titleWidth := width - statusWidth - idWidth - 6
	if titleWidth < 20 {
		titleWidth = 20
	}
	p.table.SetColumns([]table.Column{
		{Title: "Status", Width: statusWidth},
		{Title: "ID", Width: idWidth},
		{Title: "Title", Width: titleWidth},
	})
	p.table.SetHeight(max(5, height/2))
}

func (p *pinView) statusLine() string {
	if p.err != "" {
		return fmt.Sprintf("Error: %s", p.err)
	}
	if p.status != "" {
		return p.status
	}
	return ""
}

func (p *pinView) tableWithDetail() string {
	left := p.table.View()
	detail := p.detailView()
	if p.width > 0 {
		detail = lipgloss.NewStyle().Width(p.width).Render(detail)
	}
	return left + "\n\n" + detail
}

func (p *pinView) detailView() string {
	item := p.selectedItem()
	if item == nil {
		return "No item selected."
	}
	return strings.Join(item.Lines, "\n")
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
	return nil
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

func (p *pinView) runMoveChecked() {
	ids, err := pin.MoveCheckedToDone(p.files.QueuePath, p.files.DonePath, false)
	if err != nil {
		p.err = err.Error()
		p.status = ""
		return
	}
	p.err = ""
	summary := pin.SummarizeIDs(ids)
	if summary == "" {
		p.status = "No checked items moved."
	} else {
		p.status = fmt.Sprintf("Moved: %s", summary)
	}
	_ = p.reload()
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
}

func (p *pinView) finishBlock() {
	item := p.selectedItem()
	if item == nil {
		p.err = "No queue item selected."
		p.status = ""
		p.mode = pinModeTable
		return
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
		return
	}
	ok, err := pin.BlockItem(p.files.QueuePath, item.ID, reasonLines, pin.Metadata{})
	if err != nil {
		p.err = err.Error()
		p.status = ""
		p.mode = pinModeTable
		return
	}
	if !ok {
		p.err = fmt.Sprintf("Item %s not found in Queue.", item.ID)
		p.status = ""
		p.mode = pinModeTable
		return
	}
	p.status = fmt.Sprintf("Blocked %s", item.ID)
	p.err = ""
	p.mode = pinModeTable
	_ = p.reload()
}

func (p *pinView) SetConfig(cfg config.Config, locations paths.Locations) error {
	p.config = cfg
	p.locations = locations
	p.files = pin.ResolveFiles(cfg.Paths.PinDir, locations.RepoRoot)
	return p.reload()
}

func (p *pinView) RefreshIfNeeded() {
	if p.mode != pinModeTable {
		return
	}
	modTime, changed, err := fileChanged(p.files.QueuePath, p.queueMTime)
	if err != nil {
		return
	}
	if changed {
		p.queueMTime = modTime
		_ = p.reload()
	}
}

func (p *pinView) Focus() {
	p.table.Focus()
}

func (p *pinView) Blur() {
	p.table.Blur()
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
