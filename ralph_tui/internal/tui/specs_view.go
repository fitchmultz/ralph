// Package tui provides the Build Specs screen for the Ralph TUI.
// Entrypoint: specsView.
package tui

import (
	"fmt"
	"path/filepath"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/key"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/glamour"
	"github.com/mitchfultz/ralph/ralph_tui/internal/config"
	"github.com/mitchfultz/ralph/ralph_tui/internal/paths"
	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
	"github.com/mitchfultz/ralph/ralph_tui/internal/specs"
)

type specsView struct {
	cfg               config.Config
	locations         paths.Locations
	runner            specs.Runner
	interactive       bool
	innovate          bool
	innovateExplicit  bool
	autofillScout     bool
	autofillExplicit  bool
	effectiveInnovate bool
	autoEnabled       bool
	preview           string
	previewErr        string
	status            string
	err               string
	previewViewport   viewport.Model
	logViewport       viewport.Model
	previewWidth      int
	running           bool
	diffStat          string
	runLogs           []string
	lastRunOutput     string
	logCh             chan string
	pendingResult     *specsBuildResultMsg
	queueMTime        time.Time
	promptMTime       time.Time
}

type specsBuildResultMsg struct {
	err       error
	pinErr    error
	diffStat  string
	effective bool
}

type specsLogMsg struct {
	line string
}

type specsLogDoneMsg struct{}

func newSpecsView(cfg config.Config, locations paths.Locations) (*specsView, error) {
	vp := viewport.New(80, 20)
	logViewport := viewport.New(80, 20)
	view := &specsView{
		cfg:             cfg,
		locations:       locations,
		runner:          specs.RunnerCodex,
		autofillScout:   cfg.Specs.AutofillScout,
		previewViewport: vp,
		logViewport:     logViewport,
		previewWidth:    80,
	}
	view.refreshPreview()
	if modTime, err := fileModTime(filepath.Join(cfg.Paths.PinDir, "implementation_queue.md")); err == nil {
		view.queueMTime = modTime
	}
	if modTime, err := fileModTime(filepath.Join(cfg.Paths.PinDir, "specs_builder.md")); err == nil {
		view.promptMTime = modTime
	}
	return view, nil
}

func (s *specsView) Update(msg tea.Msg, keys keyMap) tea.Cmd {
	switch msg := msg.(type) {
	case specsBuildResultMsg:
		s.pendingResult = &msg
		if s.logCh == nil {
			s.applyBuildResult(msg)
			s.pendingResult = nil
		}
		return nil
	case specsLogMsg:
		s.appendRunLog(msg.line)
		if s.logCh != nil {
			return listenSpecsLogs(s.logCh)
		}
		return nil
	case specsLogDoneMsg:
		s.logCh = nil
		s.finalizeRunOutput()
		if s.pendingResult != nil {
			s.applyBuildResult(*s.pendingResult)
			s.pendingResult = nil
		}
		s.running = false
		return nil
	case tea.KeyMsg:
		switch {
		case key.Matches(msg, keys.ToggleInteractive) && !s.running:
			s.interactive = !s.interactive
			s.refreshPreview()
		case key.Matches(msg, keys.ToggleInnovate) && !s.running:
			s.innovate = !s.innovate
			s.innovateExplicit = true
			s.refreshPreview()
		case key.Matches(msg, keys.ToggleAutofill) && !s.running:
			s.autofillScout = !s.autofillScout
			s.autofillExplicit = true
			s.refreshPreview()
		case key.Matches(msg, keys.RunSpecs):
			if s.running {
				return nil
			}
			s.running = true
			s.status = "Running specs build..."
			s.err = ""
			return s.runBuildCmd()
		case msg.String() == "j":
			s.activeViewport().LineDown(1)
			return nil
		case msg.String() == "k":
			s.activeViewport().LineUp(1)
			return nil
		}
	}

	if s.running {
		updated, cmd := s.logViewport.Update(msg)
		s.logViewport = updated
		return cmd
	}
	updated, cmd := s.previewViewport.Update(msg)
	s.previewViewport = updated
	return cmd
}

func (s *specsView) View() string {
	header := "Build Specs"
	status := s.statusLine()
	options := s.optionsView()
	preview := s.previewView()
	body := strings.TrimSpace(header + "\n" + status + "\n\n" + options + "\n\n" + preview)
	return body + "\n"
}

func (s *specsView) statusLine() string {
	if s.err != "" {
		return fmt.Sprintf("Error: %s", s.err)
	}
	if s.status != "" {
		return s.status
	}
	return ""
}

func (s *specsView) optionsView() string {
	innovate := "off"
	if s.effectiveInnovate {
		innovate = "on"
		if s.autoEnabled {
			innovate += " (auto)"
		}
	}
	lines := []string{
		fmt.Sprintf("Runner: %s", s.runner),
		fmt.Sprintf("Interactive: %s", yesNo(s.interactive)),
		fmt.Sprintf("Innovate: %s", innovate),
		fmt.Sprintf("Autofill scout: %s", yesNo(s.autofillScout)),
		"Keys: i interactive | n innovate | a autofill | r run build",
		"Scroll: \u2191/\u2193 PgUp/PgDn (Ctrl+F to focus)",
	}
	return strings.Join(lines, "\n")
}

func (s *specsView) previewView() string {
	if s.running {
		return s.logViewport.View()
	}
	if s.previewErr != "" {
		return fmt.Sprintf("Prompt preview error: %s", s.previewErr)
	}
	if s.preview == "" {
		return "Prompt preview unavailable."
	}
	return s.previewViewport.View()
}

func (s *specsView) refreshPreview() {
	queuePath := filepath.Join(s.cfg.Paths.PinDir, "implementation_queue.md")
	effective, err := specs.ResolveInnovate(queuePath, s.innovate, s.innovateExplicit, s.autofillScout)
	if err != nil {
		s.previewErr = err.Error()
		s.preview = ""
		return
	}
	s.effectiveInnovate = effective
	s.autoEnabled = !s.innovateExplicit && s.autofillScout && !s.innovate && effective

	promptPath := filepath.Join(s.cfg.Paths.PinDir, "specs_builder.md")
	prompt, err := specs.FillPrompt(promptPath, s.interactive, effective)
	if err != nil {
		s.previewErr = err.Error()
		s.preview = ""
		return
	}
	renderer, err := s.buildRenderer()
	if err != nil {
		s.previewErr = err.Error()
		s.preview = ""
		return
	}
	rendered, err := renderer.Render(prompt)
	if err != nil {
		s.previewErr = err.Error()
		s.preview = ""
		return
	}
	if s.lastRunOutput != "" {
		rendered = rendered + "\n\nBuild output:\n" + s.lastRunOutput
	}
	if s.diffStat != "" {
		rendered = rendered + "\n\nDiff stat:\n" + s.diffStat
	}
	s.previewErr = ""
	s.preview = rendered
	s.previewViewport.SetContent(rendered)
	s.previewViewport.GotoTop()
}

func (s *specsView) runBuildCmd() tea.Cmd {
	s.resetRunLogs()
	logCh := make(chan string, 1024)
	s.logCh = logCh
	s.pendingResult = nil
	s.logViewport.SetContent("")
	s.logViewport.GotoTop()

	sink := logChannelSink{ch: logCh}
	writer := newStreamWriter(sink)

	runCmd := func() tea.Msg {
		defer close(logCh)
		result, err := specs.Build(specs.BuildOptions{
			RepoRoot:         s.locations.RepoRoot,
			PinDir:           s.cfg.Paths.PinDir,
			Runner:           s.runner,
			RunnerArgs:       []string{},
			Interactive:      s.interactive,
			Innovate:         s.innovate,
			InnovateExplicit: s.innovateExplicit,
			AutofillScout:    s.autofillScout,
			Stdout:           writer,
			Stderr:           writer,
		})
		writer.Flush()
		if err != nil {
			return specsBuildResultMsg{err: err}
		}
		files := pin.ResolveFiles(s.cfg.Paths.PinDir)
		pinErr := pin.ValidatePin(files)
		diffStat, diffErr := specs.GitDiffStat(s.locations.RepoRoot)
		if diffErr != nil {
			diffStat = ""
		}
		return specsBuildResultMsg{pinErr: pinErr, diffStat: diffStat, effective: result.EffectiveInnovate}
	}

	return tea.Batch(runCmd, listenSpecsLogs(logCh))
}

func yesNo(value bool) string {
	if value {
		return "on"
	}
	return "off"
}

func (s *specsView) Resize(width int, height int) {
	if width <= 0 || height <= 0 {
		return
	}
	s.previewViewport.Width = width
	s.logViewport.Width = width
	s.previewWidth = width

	const optionsLines = 6
	reserved := 1 + 1 + 2 + optionsLines + 1
	previewHeight := height - reserved
	if previewHeight < 5 {
		previewHeight = 5
	}
	s.previewViewport.Height = previewHeight
	s.logViewport.Height = previewHeight
	s.refreshPreview()
}

func (s *specsView) buildRenderer() (*glamour.TermRenderer, error) {
	wrapWidth := s.previewWidth
	if wrapWidth <= 0 {
		wrapWidth = 80
	}
	return glamour.NewTermRenderer(
		glamour.WithAutoStyle(),
		glamour.WithWordWrap(wrapWidth),
	)
}

func (s *specsView) SetConfig(cfg config.Config, locations paths.Locations) {
	s.cfg = cfg
	s.locations = locations
	if !s.autofillExplicit {
		s.autofillScout = cfg.Specs.AutofillScout
	}
	if modTime, err := fileModTime(filepath.Join(cfg.Paths.PinDir, "implementation_queue.md")); err == nil {
		s.queueMTime = modTime
	}
	if modTime, err := fileModTime(filepath.Join(cfg.Paths.PinDir, "specs_builder.md")); err == nil {
		s.promptMTime = modTime
	}
	if !s.running {
		s.refreshPreview()
	}
}

func (s *specsView) RefreshIfNeeded() {
	if s.running {
		return
	}
	queuePath := filepath.Join(s.cfg.Paths.PinDir, "implementation_queue.md")
	queueTime, queueChanged, queueErr := fileChanged(queuePath, s.queueMTime)
	if queueErr != nil {
		return
	}
	promptPath := filepath.Join(s.cfg.Paths.PinDir, "specs_builder.md")
	promptTime, promptChanged, promptErr := fileChanged(promptPath, s.promptMTime)
	if promptErr != nil {
		return
	}
	if queueChanged {
		s.queueMTime = queueTime
	}
	if promptChanged {
		s.promptMTime = promptTime
	}
	if queueChanged || promptChanged {
		s.refreshPreview()
	}
}

func (s *specsView) Focus() {}

func (s *specsView) Blur() {}

func (s *specsView) activeViewport() *viewport.Model {
	if s.running {
		return &s.logViewport
	}
	return &s.previewViewport
}

func (s *specsView) resetRunLogs() {
	s.runLogs = nil
	s.lastRunOutput = ""
	s.diffStat = ""
}

func (s *specsView) appendRunLog(line string) {
	const maxLines = 500
	s.runLogs = append(s.runLogs, line)
	if len(s.runLogs) > maxLines {
		s.runLogs = s.runLogs[len(s.runLogs)-maxLines:]
	}
	atBottom := s.logViewport.AtBottom()
	s.logViewport.SetContent(strings.Join(s.runLogs, "\n"))
	if atBottom {
		s.logViewport.GotoBottom()
	}
}

func (s *specsView) finalizeRunOutput() {
	if len(s.runLogs) == 0 {
		s.lastRunOutput = ""
		return
	}
	s.lastRunOutput = strings.Join(s.runLogs, "\n")
}

func (s *specsView) applyBuildResult(msg specsBuildResultMsg) {
	if msg.err != nil {
		s.err = msg.err.Error()
		s.status = ""
		s.refreshPreview()
		return
	}
	if msg.pinErr != nil {
		s.err = msg.pinErr.Error()
		s.status = ""
		s.refreshPreview()
		return
	}
	s.err = ""
	s.status = ">> [RALPH] Pin validation OK."
	s.diffStat = msg.diffStat
	s.refreshPreview()
}

type logChannelSink struct {
	ch chan<- string
}

func (s logChannelSink) PushLine(line string) {
	if s.ch == nil {
		return
	}
	select {
	case s.ch <- line:
	default:
	}
}

func listenSpecsLogs(logCh <-chan string) tea.Cmd {
	return func() tea.Msg {
		line, ok := <-logCh
		if !ok {
			return specsLogDoneMsg{}
		}
		return specsLogMsg{line: line}
	}
}
