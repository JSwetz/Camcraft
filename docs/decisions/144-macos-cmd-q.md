## #144 - macOS Cmd+Q asks about unsaved changes (2026-11-08)

- **Finding (winit 0.30.13 source):** the default macOS app menu's Quit
  item (Cmd+Q) sends `terminate:` to NSApplication
  (`platform_impl/macos/menu.rs`). winit's application delegate
  implements only `applicationDidFinishLaunching:` and
  `applicationWillTerminate:` (`app_state.rs`) - no
  `applicationShouldTerminate:` - so AppKit terminates at once: winit
  closes the windows, the process exits, `run_app` never returns, and no
  `CloseRequested` reaches the shell. The #107 prompt was skipped, as the
  PR #141 review suspected. The menu also eats the Cmd+Q key, so the
  egui Cmd/Ctrl+Q shortcut never saw it either. The Dock's Quit, a
  `quit` Apple Event and a logout take the same `terminate:` path.
- **Chosen:** add `applicationShouldTerminate:` to winit's delegate class
  at startup (`egui_app/os_quit.rs`, `class_addMethod` through `objc2`
  0.5 - the runtime binding winit already uses, same version, no new
  crate in the lock; macOS-only dependency). It answers
  `NSTerminateCancel` and sets a flag; the shell's `about_to_wait` turns
  the flag into `actions::quit` - the same guarded Quit command the
  window X and Ctrl+Q queue - and brings the window forward
  (un-minimise, focus) so the prompt is seen. A clean document quits at
  once through the normal `el.exit()` path (which stops the app, not
  `terminate:`, so the hook does not re-enter). One mechanism covers
  Cmd+Q, the app menu, the Dock and logout.
- **Robustness:** `class_addMethod` only adds - if a future winit gives
  its delegate its own `applicationShouldTerminate:`, the call returns NO,
  the shell logs that and keeps winit's behaviour. A missing app or
  delegate is logged the same way. The hook only stores an atomic flag,
  so nothing can unwind across the FFI boundary. A shell whose GPU init
  failed (draws nothing) exits on the request, like its window X.
- **Logout:** cancelling `terminate:` also cancels a logout/shutdown
  while the app is open (macOS names the app that stopped it). That is
  the standard document-app behaviour and better than losing work; the
  user quits (answering the prompt) and logs out again. Not done:
  `NSTerminateLater` + `replyToApplicationShouldTerminate:` so a logout
  resumes after the answer - more FFI for a rarer path.
- **Dropped:** retargeting the default menu's Quit item to our own
  target object (covers Cmd+Q and the menu only, not the Dock or logout,
  and needs a declared class); `with_default_menu(false)` plus our own
  menu (more code, the Dock/logout path still unguarded); retargeting
  Quit to `performClose:` (does nothing when no window is key, e.g.
  minimised).
- **Verified here:** a launched debug build, sent a quit through
  `NSRunningApplication.terminate` (the `terminate:` path), routed it
  through the hook and left `run_app` normally (exit 0) with a clean
  document. The dirty-document prompt is pinned headlessly
  (`os_quit_request_over_unsaved_changes_prompts_once`); the real Cmd+Q
  check over unsaved work is a contributor's on a Mac.
- **Cost to change later:** one file; removing it restores winit's
  behaviour. A winit upgrade that adds its own handler makes the hook
  log and stand down - re-check then.
