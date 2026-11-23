# desktop-app Specification

## Purpose

Provides desktop application startup and system tray residency, global capture shortcuts, capture history, settings and localization, multi-window lifecycle management, and external studio integration.

## Requirements

### Requirement: Application Startup and System Tray Residency
The desktop application SHALL create a system tray icon with menu items (`Show Editor`, `Capture Screen`, `Cancel Capture`, `Open Image...`, `Quit`) and left-click restore. Ordinary launches SHALL open the main window on the Home view; launching with `--background` or `--capture` SHALL keep the main window hidden initially.

#### Scenario: Ordinary launch vs background tray launch
- **WHEN** user launches MarkIts normally versus with `--background`
- **THEN** ordinary launch opens the main window with tray residency, while `--background` stays hidden in the tray until invoked

#### Scenario: Quit from tray context menu
- **WHEN** user clicks "Quit" in the system tray context menu
- **THEN** any active recording is canceled and the application exits cleanly

### Requirement: Global Shortcut Registration and Settings
The system SHALL register a configurable global capture shortcut (default `Control+Shift+E`, supporting modifier combinations, function keys `F1`-`F24`, and `PrintScreen` while rejecting plain typing keys), debounce key-hold repeats, warn on registration conflict, keep the shortcut active after the editor hides, and expose shortcut, language (`ja`/`en`), and UI exploration depth (`1`-`128`) settings in the toolbar settings popover.

#### Scenario: Trigger capture via global shortcut
- **WHEN** user presses the configured global shortcut while another application has focus
- **THEN** MarkIts triggers capture (or completes a pending cursor-capture wait / recording screenshot) without repeating while held

#### Scenario: Handle shortcut conflict gracefully
- **WHEN** the configured shortcut is already reserved by the OS or another app
- **THEN** MarkIts surfaces a warning in the settings panel and allows changing the shortcut

### Requirement: Capture History Gallery
The desktop application SHALL automatically save edited captures to local history and display them in the Home view with thumbnails, crop/annotation indicators, grouped operation-recording frames, re-edit, quick clipboard copy, and deletion.

#### Scenario: Re-edit capture from Home history
- **WHEN** user clicks a history card in the Home view
- **THEN** the editor opens with the saved background, annotations, UIMap, and crop state restored

### Requirement: Window Lifecycle Management
The system SHALL manage distinct windows (`main`, `overlay`, `cursor-guide`, `recording`, `playback`) so that closing `main` in standalone mode hides it to the tray while keeping shortcuts active, and closing `overlay`, `cursor-guide`, `recording`, or `playback` cleanly cancels its active workflow.

#### Scenario: Transition from capture to editor window
- **WHEN** a capture completes from the overlay or cursor guide
- **THEN** the capture window closes/hides, the editor window opens with the captured image, and the editor gains focus

#### Scenario: Closing main window in standalone mode
- **WHEN** user closes the main window in standalone mode
- **THEN** the window hides while MarkIts remains active in the system tray with its shortcut registered

### Requirement: External Integration Mode
When launched with Manual Studio integration arguments (`--manual-studio-output`, `--manual-studio-completion`, and either `--manual-studio-input` or `--capture`), the app SHALL enter integration mode, save and write the completion marker on "編集終了" before exiting, and write a `.exit` cancellation marker and exit cleanly if the editor is closed or capture is canceled before editing begins.

#### Scenario: Complete or cancel external integration session
- **WHEN** user clicks "編集終了" in integration mode, or closes the window without completing
- **THEN** "編集終了" writes the output image and completion marker and exits, whereas closing writes the `.exit` marker and exits without hiding to tray
