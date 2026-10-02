// Sends the focused window's resource class to the keycap-focus service on the session bus.
// The service decides the profile; this script only reports.
function notifyActiveWindow(window) {
    // null while the task switcher or a shell surface has focus: keep the last profile.
    if (!window) {
        return;
    }
    callDBus(
        "org.keycap.Focus",
        "/org/keycap/Focus",
        "org.keycap.Focus",
        "Focus",
        window.resourceClass ? String(window.resourceClass) : ""
    );
}

workspace.windowActivated.connect(notifyActiveWindow);
notifyActiveWindow(workspace.activeWindow);
