// Package tui provides the Task Builder screen for the Ralph TUI.
// Entrypoint: taskBuilderView.
package tui

import (
	"context"
	"fmt"
	"strings"

	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/huh"
	"github.com/mitchfultz/ralph/ralph_tui/internal/config"
	"github.com/mitchfultz/ralph/ralph_tui/internal/paths"
	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
	"github.com/mitchfultz/ralph/ralph_tui/internal/runnerargs"
	"github.com/mitchfultz/ralph/ralph_tui/internal/taskbuilder"
)

type taskBuilderView struct {
	cfg       config.Config
	locations paths.Locations
	form      *huh.Form
	preview   string
	status    string
	err       string
	width     int
	height    int
	building  bool
	parentCtx context.Context

	prompt      string
	tags        string
	scope       string
	description string
	runner      string
	effort      string
	queueNow    bool
	runNow      bool
	insertAtTop bool

	previewViewport viewport.Model
}

type taskBuilderResultMsg struct {
	id        string
	itemBlock []string
	err       error
	runNow    bool
	runner    string
	effort    string
	tags      []string
	queued    bool
}

type taskBuilderQueuedMsg struct {
	ID     string
	Runner string
	Effort string
	Tags   []string
}

func newTaskBuilderView(cfg config.Config, locations paths.Locations, keys keyMap) (*taskBuilderView, error) {
	vp := viewport.New(80, 20)
	vp.Style = paddedViewportStyle

	view := &taskBuilderView{
		cfg:             cfg,
		locations:       locations,
		parentCtx:       context.Background(),
		scope:           "repo",
		queueNow:        true,
		runNow:          false,
		insertAtTop:     false,
		previewViewport: vp,
		status:          "Enter a prompt and submit to build a queue item.",
	}
	view.applyConfigDefaults(cfg)
	view.form = view.buildForm()
	return view, nil
}

func (v *taskBuilderView) Update(msg tea.Msg, keys keyMap) tea.Cmd {
	switch msg := msg.(type) {
	case taskBuilderResultMsg:
		v.building = false
		if msg.err != nil {
			v.err = msg.err.Error()
			v.status = "Build failed"
			return nil
		}
		v.err = ""
		v.preview = strings.Join(msg.itemBlock, "\n")
		v.previewViewport.SetContent(v.preview)
		v.previewViewport.GotoTop()
		if msg.queued {
			v.status = fmt.Sprintf("Queued %s", msg.id)
		} else {
			v.status = "Preview ready"
		}
		if msg.runNow {
			tags := msg.tags
			if len(tags) == 0 && len(msg.itemBlock) > 0 {
				tags = pin.ExtractTags(msg.itemBlock[0])
			}
			return func() tea.Msg {
				return taskBuilderQueuedMsg{
					ID:     msg.id,
					Runner: msg.runner,
					Effort: msg.effort,
					Tags:   tags,
				}
			}
		}
		return nil
	case tea.KeyMsg:
		if v.building {
			return nil
		}
		if v.form != nil {
			model, cmd := v.form.Update(msg)
			if form, ok := model.(*huh.Form); ok {
				v.form = form
			}
			switch v.form.State {
			case huh.StateCompleted:
				v.form = v.buildForm()
				return v.startBuild()
			case huh.StateAborted:
				v.form = v.buildForm()
				v.status = "Build cancelled"
				return nil
			default:
				return cmd
			}
		}
	}
	if v.form != nil {
		model, cmd := v.form.Update(msg)
		if form, ok := model.(*huh.Form); ok {
			v.form = form
		}
		return cmd
	}
	return nil
}

func (v *taskBuilderView) View() string {
	header := "Task Builder"
	status := v.statusLine()
	formView := ""
	if v.form != nil {
		formView = v.form.View()
	}
	preview := v.previewViewport.View()
	previewContent := v.preview
	body := header + "\n" + status
	if formView != "" {
		body += "\n\n" + formView
	}
	if preview != "" {
		body += "\n\nQueue Preview\n" + preview
	}
	if preview == "" && previewContent != "" {
		body += "\n\n[DEBUG: viewport blank but content not empty, len=" + fmt.Sprint(len(previewContent)) + "]"
	}
	return withFinalNewline(body)
}

func (v *taskBuilderView) Resize(width int, height int) {
	v.width = width
	v.height = height
	if v.form == nil {
		return
	}
	header := "Task Builder"
	status := v.statusLine()
	chrome := chromeHeight(
		width,
		wrappedBlock{Text: header, MinRows: 1},
		wrappedBlock{Text: status, MinRows: 1, BlankLinesAfter: 1},
	)
	remaining := remainingHeight(height, chrome)
	formHeight, previewHeight := splitTwoPaneHeight(remaining, 3, 5)
	v.form = resizeHuhFormToFit(v.form, width, formHeight)
	resizeViewportToFit(&v.previewViewport, width, previewHeight, paddedViewportStyle)
}

func (v *taskBuilderView) Focus() {}

func (v *taskBuilderView) Blur() {}

func (v *taskBuilderView) IsTyping() bool {
	if v == nil || v.form == nil {
		return false
	}
	focused := v.form.GetFocusedField()
	switch focused.(type) {
	case *huh.Input, *huh.Text:
		return true
	default:
		return false
	}
}

func (v *taskBuilderView) SetConfig(cfg config.Config, locations paths.Locations) {
	v.cfg = cfg
	v.locations = locations
	v.applyConfigDefaults(cfg)
}

func (v *taskBuilderView) applyConfigDefaults(cfg config.Config) {
	if strings.TrimSpace(v.runner) == "" {
		v.runner = runnerargs.NormalizeRunner(cfg.Loop.Runner)
	}
	normalizedEffort := runnerargs.NormalizeEffort(cfg.Loop.ReasoningEffort)
	switch normalizedEffort {
	case "low", "medium", "high":
		v.effort = normalizedEffort
	default:
		if v.effort == "" {
			v.effort = "medium"
		}
	}
}

func (v *taskBuilderView) statusLine() string {
	parts := []string{}
	if v.status != "" {
		parts = append(parts, v.status)
	}
	if v.err != "" {
		parts = append(parts, "Error: "+v.err)
	}
	if len(parts) == 0 {
		return ""
	}
	return strings.Join(parts, " | ")
}

func (v *taskBuilderView) buildForm() *huh.Form {
	promptField := huh.NewText().
		Title("Prompt").
		Value(&v.prompt).
		Lines(4)
	tagsField := huh.NewInput().
		Title("Routing Tags (comma/space-separated)").
		Value(&v.tags)
	scopeField := huh.NewInput().
		Title("Scope (no parentheses needed)").
		Value(&v.scope)
	descriptionField := huh.NewInput().
		Title("Description (optional)").
		Value(&v.description)
	runnerField := huh.NewSelect[string]().
		Title("Runner").
		Options(
			huh.NewOption("codex", "codex"),
			huh.NewOption("opencode", "opencode"),
		).
		Value(&v.runner)
	effortField := huh.NewSelect[string]().
		Title("Reasoning Effort").
		Options(
			huh.NewOption("low", "low"),
			huh.NewOption("medium", "medium"),
			huh.NewOption("high", "high"),
		).
		Value(&v.effort)
	topField := huh.NewConfirm().
		Title("High Priority (Insert at Top)").
		Value(&v.insertAtTop)
	queueField := huh.NewConfirm().
		Title("Queue item now").
		Value(&v.queueNow)
	runField := huh.NewConfirm().
		Title("Run one loop iteration after queueing").
		Value(&v.runNow)

	return huh.NewForm(
		huh.NewGroup(
			promptField,
			tagsField,
			scopeField,
			descriptionField,
			runnerField,
			effortField,
			topField,
			queueField,
			runField,
		),
	).WithShowHelp(false)
}

func (v *taskBuilderView) startBuild() tea.Cmd {
	if v.building {
		return nil
	}
	prompt := strings.TrimSpace(v.prompt)
	if prompt == "" {
		v.err = "Prompt is required"
		v.status = "Build failed"
		return nil
	}
	if v.runNow && !v.queueNow {
		v.err = "Run now requires queueing the item"
		v.status = "Build failed"
		return nil
	}

	tags := []string{}
	if strings.TrimSpace(v.tags) != "" {
		parsed, err := pin.ValidateTagList("task builder tags", v.tags)
		if err != nil {
			v.err = err.Error()
			v.status = "Build failed"
			return nil
		}
		tags = parsed
	}

	v.building = true
	v.status = "Building..."
	v.err = ""

	buildOpts := taskbuilder.BuildOptions{
		RepoRoot:     v.locations.RepoRoot,
		PinDir:       v.cfg.Paths.PinDir,
		ProjectType:  v.cfg.ProjectType,
		Prompt:       prompt,
		Tags:         tags,
		Scope:        v.scope,
		Description:  v.description,
		WriteToQueue: v.queueNow,
		InsertAtTop:  v.insertAtTop,
	}

	runNow := v.runNow
	runner := runnerargs.NormalizeRunner(v.runner)
	effort := runnerargs.NormalizeEffort(v.effort)

	return func() tea.Msg {
		result, err := taskbuilder.Build(v.parentCtx, buildOpts)
		return taskBuilderResultMsg{
			id:        result.ID,
			itemBlock: result.ItemBlock,
			err:       err,
			runNow:    runNow,
			runner:    runner,
			effort:    effort,
			tags:      tags,
			queued:    v.queueNow,
		}
	}
}
