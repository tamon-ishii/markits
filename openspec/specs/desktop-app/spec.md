# desktop-app Specification

## Purpose

Provides background system tray residency, tray context menu options, global shortcut registration (including PrintScreen), and window lifecycle management for the MarkIts desktop application.

## Requirements

### Requirement: System Tray Residency and Context Menu
The system SHALL launch as a background desktop application residing in the system notification area (system tray) with a custom tray icon and a context menu providing options to trigger a capture, open an existing image, configure settings, and quit the application.

#### Scenario: Background tray residency
- **WHEN** user launches the MarkIts desktop application
- **THEN** an icon appears in the system tray, the application remains running in the background without keeping a main window open, and the tray menu is accessible

#### Scenario: Quit from tray context menu
- **WHEN** user clicks "Quit" in the system tray context menu
- **THEN** the application terminates all active windows and shuts down cleanly

### Requirement: Global Shortcut Registration
The system SHALL register global shortcuts (including PrintScreen key and configurable combinations) to trigger the screen capture overlay from any application context.

#### Scenario: Trigger capture via global shortcut
- **WHEN** user presses the configured global shortcut (such as PrintScreen) while another application has focus
- **THEN** the system intercepts the keystroke and immediately invokes the screen capture overlay

#### Scenario: Handle shortcut conflict gracefully
- **WHEN** the configured shortcut is already reserved by the operating system or another application
- **THEN** the application notifies the user with a warning and provides an interface to change the shortcut keybinding

#### Scenario: Keep shortcut after editor closes
- **WHEN** the user closes the editor window while MarkIts remains in the tray
- **THEN** the selected capture shortcut remains registered

### Requirement: Window Lifecycle Management
The system SHALL manage distinct windows for the capture overlay and the annotation editor, ensuring windows are opened, focused, and closed without terminating the background tray process.

#### Scenario: Transition from capture to editor window
- **WHEN** a screen capture region or window is selected and captured
- **THEN** the capture overlay window closes, the editor window opens with the captured image loaded, and the editor gains input focus

#### Scenario: Closing the editor window
- **WHEN** user closes the annotation editor window
- **THEN** the editor window closes or hides while the application remains active in the system tray
