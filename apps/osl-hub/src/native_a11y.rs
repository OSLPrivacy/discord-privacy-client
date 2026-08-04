//! Windows accessibility primitives, and the shared UIA2 substrate: a window
//! taxonomy plus the producer that feeds it real windows.
//!
//! # What is proven, and by what
//!
//! `Uia2WindowShape`, `Uia2WindowPlan` and `resolve_uia2_window` record the
//! three window shapes A-00 measured, and `resolve_uia2_window` is a pure
//! function over an already-enumerated window graph. Below the taxonomy,
//! `acquire_uia2_window` is the producer that enumerates, resolves the shape,
//! dispatches the wake keyed on `wake_policy`, polls until the tree populates,
//! and hands back a `Uia2ResolvedWindow` whose every cross-process call is
//! issued under `call_timeout_ms`. `resolve_uia2_composer` and
//! `place_uia2_carrier` complete the path A-00 drove from PowerShell.
//!
//! The pipeline is portable and is exercised off Windows against recorded
//! window graphs for all four measured providers. The one part that is not is
//! `win32`, the `Uia2Syscalls` implementor: it is `cfg(target_os = "windows")`,
//! so a Linux build neither compiles nor tests it, and **nothing in it has been
//! run against a live provider from Rust**. Cross-compile to
//! `x86_64-pc-windows-gnu` before believing it builds, and read
//! `tasklogs/A-00b.md` for exactly what is and is not verified.
//!
//! Discord is the one live consumer. It consumes the shape decision --
//! `resolve_uia2_wake_target` chooses which window its wake is issued at -- and
//! keeps its own MSAA walk, poll ladder and synthetic-input write path, which
//! are the only end-to-end proven mechanism in the product. It deliberately does
//! **not** adopt `acquire_uia2_window`'s poll or `place_uia2_carrier`: both
//! would change its timing, and changing Discord's behaviour is a regression,
//! not a refactor.
//!
//! # Chromium's lazy accessibility tree
//!
//! Chromium enables its accessibility tree lazily.  Its documented handshake is
//! an `EVENT_SYSTEM_ALERT` for custom object id 1, followed by `WM_GETOBJECT`
//! for that same object id.  `OBJID_CLIENT` alone is not that handshake.

/// Chromium's accessibility-presence event.
pub(crate) const EVENT_SYSTEM_ALERT: u32 = 0x0002;

/// Chromium's documented custom accessibility object id.
pub(crate) const ELECTRON_A11Y_OBJECT_ID: i32 = 1;

/// Electron's top-level Chromium host window class.
pub const ELECTRON_OUTER_WINDOW_CLASS: &str = "Chrome_WidgetWin_1";

/// Electron's content-bearing Chromium renderer child window class.
pub const ELECTRON_RENDERER_WINDOW_CLASS: &str = "Chrome_RenderWidgetHostHWND";

/// Telegram Desktop's Qt top-level window class. There is no Chromium renderer.
pub const TELEGRAM_OUTER_WINDOW_CLASS: &str = "Qt51519QWindowIcon";

/// WhatsApp Desktop's WinUI shell window. The useful content is not below it.
pub const WHATSAPP_OUTER_WINDOW_CLASS: &str = "WinUIDesktopWin32WindowClass";

/// WhatsApp's WebView2 content process.
pub const WEBVIEW2_PROCESS_NAME: &str = "msedgewebview2";

/// A UIA tree below this size is treated as not yet asynchronously populated.
pub const ELECTRON_UIA2_POPULATED_MIN_ELEMENTS: usize = 10;

/// Measured UIA2 window shapes for providers OSL drives through native a11y.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Uia2WindowShape {
    /// App outer window -> `Chrome_RenderWidgetHostHWND`, wake, then poll.
    ChromiumRendererChild,
    /// The app's outer window is the UIA root; no renderer child is expected.
    DirectOuterWindow,
    /// App shell proves ownership, then bind a sibling WebView2 Chromium host.
    SiblingChromiumRenderer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Uia2WakePolicy {
    None,
    WmGetObjectChromium,
}

/// How the accessibility tree under a bound window is reached.
///
/// The window shape says *which* window; this says *through what*. They are
/// independent axes and conflating them is what made "bind the outer Chromium
/// window" look like a single decision when it is two.
///
/// A-00 measured the [`Uia2TreeRoute::UiaNative`] route on all four providers
/// from PowerShell. Discord's shipping Rust path is the other one: it never
/// binds the renderer child, it wakes the *outer* window and takes Chromium's
/// custom MSAA client object, then bridges that object into UI Automation. Both
/// reach a writable composer; they are not interchangeable, and the outer
/// window is only a mistake on the `UiaNative` route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Uia2TreeRoute {
    /// Ask UI Automation for the element at the bound window and walk from it.
    UiaNative,
    /// Wake Chromium at the bound window, take the MSAA client object it hands
    /// back, and bridge that into UI Automation.
    MsaaBridge,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Uia2WindowPlan {
    pub provider_name: &'static str,
    pub app_process_name: &'static str,
    pub app_outer_class: &'static str,
    pub shape: Uia2WindowShape,
    pub sibling_process_name: Option<&'static str>,
    pub sibling_outer_class: Option<&'static str>,
    pub renderer_child_class: Option<&'static str>,
    pub wake_policy: Uia2WakePolicy,
    pub tree_route: Uia2TreeRoute,
    pub poll_until_populated: bool,
    pub populated_min_elements: usize,
    pub default_wait_ms: u64,
    pub call_timeout_ms: u64,
}

impl Uia2WindowPlan {
    pub const fn chromium_renderer_child(
        provider_name: &'static str,
        app_process_name: &'static str,
        default_wait_ms: u64,
        call_timeout_ms: u64,
    ) -> Self {
        Self {
            provider_name,
            app_process_name,
            app_outer_class: ELECTRON_OUTER_WINDOW_CLASS,
            shape: Uia2WindowShape::ChromiumRendererChild,
            sibling_process_name: None,
            sibling_outer_class: None,
            renderer_child_class: Some(ELECTRON_RENDERER_WINDOW_CLASS),
            wake_policy: Uia2WakePolicy::WmGetObjectChromium,
            tree_route: Uia2TreeRoute::UiaNative,
            poll_until_populated: true,
            populated_min_elements: ELECTRON_UIA2_POPULATED_MIN_ELEMENTS,
            default_wait_ms,
            call_timeout_ms,
        }
    }

    pub const fn direct_outer_window(
        provider_name: &'static str,
        app_process_name: &'static str,
        app_outer_class: &'static str,
        call_timeout_ms: u64,
    ) -> Self {
        Self {
            provider_name,
            app_process_name,
            app_outer_class,
            shape: Uia2WindowShape::DirectOuterWindow,
            sibling_process_name: None,
            sibling_outer_class: None,
            renderer_child_class: None,
            wake_policy: Uia2WakePolicy::None,
            tree_route: Uia2TreeRoute::UiaNative,
            poll_until_populated: false,
            populated_min_elements: 1,
            default_wait_ms: 0,
            call_timeout_ms,
        }
    }

    pub const fn sibling_chromium_renderer(
        provider_name: &'static str,
        app_process_name: &'static str,
        app_outer_class: &'static str,
        sibling_process_name: &'static str,
        default_wait_ms: u64,
        call_timeout_ms: u64,
    ) -> Self {
        Self {
            provider_name,
            app_process_name,
            app_outer_class,
            shape: Uia2WindowShape::SiblingChromiumRenderer,
            sibling_process_name: Some(sibling_process_name),
            sibling_outer_class: Some(ELECTRON_OUTER_WINDOW_CLASS),
            renderer_child_class: Some(ELECTRON_RENDERER_WINDOW_CLASS),
            wake_policy: Uia2WakePolicy::WmGetObjectChromium,
            tree_route: Uia2TreeRoute::UiaNative,
            poll_until_populated: true,
            populated_min_elements: ELECTRON_UIA2_POPULATED_MIN_ELEMENTS,
            default_wait_ms,
            call_timeout_ms,
        }
    }

    /// Discord's shipping shape: the outer Chromium host window, woken with
    /// Chromium's handshake, read through the MSAA client object it hands back
    /// and bridged into UI Automation.
    ///
    /// This is **not** [`Uia2WindowPlan::chromium_outer_mutant`] with a nicer
    /// name. The mutant binds the outer window and then asks *UI Automation*
    /// for its tree, which A-00 measured as blind. This plan never asks UI
    /// Automation for a tree at that window: `wake_electron_accessibility`
    /// returns Chromium's own custom accessibility object, and that object is
    /// the root. `tree_route` is what distinguishes them, and it is the field
    /// the acquisition reads.
    pub const fn chromium_outer_msaa_root(
        provider_name: &'static str,
        app_process_name: &'static str,
        populated_min_elements: usize,
        default_wait_ms: u64,
        call_timeout_ms: u64,
    ) -> Self {
        Self {
            provider_name,
            app_process_name,
            app_outer_class: ELECTRON_OUTER_WINDOW_CLASS,
            shape: Uia2WindowShape::DirectOuterWindow,
            sibling_process_name: None,
            sibling_outer_class: None,
            renderer_child_class: None,
            wake_policy: Uia2WakePolicy::WmGetObjectChromium,
            tree_route: Uia2TreeRoute::MsaaBridge,
            poll_until_populated: true,
            populated_min_elements,
            default_wait_ms,
            call_timeout_ms,
        }
    }

    /// Deliberately wrong: binds the outer host window instead of the
    /// renderer child. For the side-by-side probe only, never for production
    /// binding. Kept public so the probe can name it.
    pub const fn chromium_outer_mutant(
        provider_name: &'static str,
        app_process_name: &'static str,
        default_wait_ms: u64,
        call_timeout_ms: u64,
    ) -> Self {
        Self {
            provider_name,
            app_process_name,
            app_outer_class: ELECTRON_OUTER_WINDOW_CLASS,
            shape: Uia2WindowShape::DirectOuterWindow,
            sibling_process_name: None,
            sibling_outer_class: None,
            renderer_child_class: None,
            wake_policy: Uia2WakePolicy::WmGetObjectChromium,
            tree_route: Uia2TreeRoute::UiaNative,
            poll_until_populated: true,
            populated_min_elements: ELECTRON_UIA2_POPULATED_MIN_ELEMENTS,
            default_wait_ms,
            call_timeout_ms,
        }
    }

    /// Deliberately wrong: binds the renderer child but skips Chromium's
    /// wake handshake. Probe only, never for production binding.
    pub const fn chromium_renderer_no_wake_mutant(
        provider_name: &'static str,
        app_process_name: &'static str,
        default_wait_ms: u64,
        call_timeout_ms: u64,
    ) -> Self {
        Self {
            provider_name,
            app_process_name,
            app_outer_class: ELECTRON_OUTER_WINDOW_CLASS,
            shape: Uia2WindowShape::ChromiumRendererChild,
            sibling_process_name: None,
            sibling_outer_class: None,
            renderer_child_class: Some(ELECTRON_RENDERER_WINDOW_CLASS),
            wake_policy: Uia2WakePolicy::None,
            tree_route: Uia2TreeRoute::UiaNative,
            poll_until_populated: true,
            populated_min_elements: ELECTRON_UIA2_POPULATED_MIN_ELEMENTS,
            default_wait_ms,
            call_timeout_ms,
        }
    }

    /// Deliberately wrong: binds and reads immediately, without waiting for
    /// the asynchronously populated tree. Probe only, never for production
    /// binding.
    pub const fn chromium_renderer_immediate_mutant(
        provider_name: &'static str,
        app_process_name: &'static str,
        call_timeout_ms: u64,
    ) -> Self {
        Self {
            provider_name,
            app_process_name,
            app_outer_class: ELECTRON_OUTER_WINDOW_CLASS,
            shape: Uia2WindowShape::ChromiumRendererChild,
            sibling_process_name: None,
            sibling_outer_class: None,
            renderer_child_class: Some(ELECTRON_RENDERER_WINDOW_CLASS),
            wake_policy: Uia2WakePolicy::WmGetObjectChromium,
            tree_route: Uia2TreeRoute::UiaNative,
            poll_until_populated: false,
            populated_min_elements: ELECTRON_UIA2_POPULATED_MIN_ELEMENTS,
            default_wait_ms: 0,
            call_timeout_ms,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Uia2WindowCandidate<'a> {
    pub hwnd: isize,
    pub parent_hwnd: Option<isize>,
    /// For out-of-process content roots this ties the sibling Chromium window
    /// back to the app shell that OSL claimed **through the window tree**.
    ///
    /// D-156 measured this as `None` on the owner's host for WhatsApp: the
    /// WebView2 content window is genuinely top-level, and `GetParent`,
    /// `GA_ROOT`, `GA_ROOTOWNER` and `GWLP_HWNDPARENT` are all zero or itself in
    /// both directions. It is kept because it is a *sound* link when it exists
    /// -- a window whose root ancestor is the shell's own handle belongs to that
    /// shell by the window manager's own arithmetic -- but it is not the link
    /// WhatsApp has, and no ancestry-derived field can be.
    pub associated_app_hwnd: Option<isize>,
    pub process_id: u32,
    pub process_name: &'a str,
    /// The process that created this window's process, read from the **process
    /// table**, not the window tree. This is the relationship WhatsApp actually
    /// has: `msedgewebview2` pid 24196 has `ParentProcessId` 23884, the
    /// `WhatsApp.Root` shell.
    ///
    /// `0` means unknown, and never matches: a candidate whose parentage could
    /// not be read is not thereby associated with anything.
    pub parent_process_id: u32,
    /// The image name a WebView2 host declares it is hosting, taken from its own
    /// `--webview-exe-name=` command-line switch.
    ///
    /// This is the corroboration, not the link. Parentage says *who launched
    /// this process*; this says *which application it is hosting a WebView for*.
    /// Requiring both is what stops a pid-reuse or an unrelated child process
    /// from being read as WhatsApp's content window.
    pub host_exe_name: Option<&'a str>,
    pub class_name: &'a str,
    pub visible: bool,
    pub area: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Uia2ResolvedWindow {
    pub app_outer_hwnd: isize,
    pub bound_hwnd: isize,
    pub bound_process_id: u32,
    pub wake_policy: Uia2WakePolicy,
    pub tree_route: Uia2TreeRoute,
    pub poll_until_populated: bool,
    pub populated_min_elements: usize,
    pub call_timeout_ms: u64,
}

/// A cross-process accessibility call that did not answer inside its deadline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Uia2CallTimeout {
    pub timeout_ms: u64,
}

/// Issue one cross-process accessibility call under a hard deadline.
///
/// A UIA or MSAA call into another process cannot be cancelled once issued: if
/// the provider stops answering, the calling thread blocks forever, and both
/// OSL and Discord have been frozen exactly that way on this machine. So the
/// call is issued on a worker thread and the caller stops waiting at the
/// deadline, returning `Err(Uia2CallTimeout)` rather than blocking. The worker
/// may still be parked in the stuck call afterwards; that is unavoidable, and
/// it is precisely why the deadline has to belong to the caller.
pub fn call_with_timeout<T: Send + 'static>(
    timeout_ms: u64,
    call: impl FnOnce() -> T + Send + 'static,
) -> Result<T, Uia2CallTimeout> {
    let (answer, wait) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = answer.send(call());
    });
    wait.recv_timeout(std::time::Duration::from_millis(timeout_ms))
        .map_err(|_| Uia2CallTimeout { timeout_ms })
}

impl Uia2ResolvedWindow {
    /// Every cross-process accessibility call against this window must be
    /// issued through here. This is what reads `call_timeout_ms`, and what
    /// makes it a deadline rather than a declaration.
    pub fn bounded_call<T: Send + 'static>(
        &self,
        call: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, Uia2CallTimeout> {
        call_with_timeout(self.call_timeout_ms, call)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Uia2WindowResolveError {
    MissingAppOuter,
    MissingRendererChild,
    MissingSiblingContentOuter,
}

/// Resolve a provider's measured UIA2 root from an already-enumerated window
/// graph. The graph boundary is kept small so Windows enumeration, process
/// trust, visibility and geometry checks can feed one shared resolver.
pub fn resolve_uia2_window(
    plan: Uia2WindowPlan,
    windows: &[Uia2WindowCandidate<'_>],
) -> Result<Uia2ResolvedWindow, Uia2WindowResolveError> {
    let app_outer = largest_visible(windows.iter().copied().filter(|window| {
        same_process_name(window.process_name, plan.app_process_name)
            && window.class_name == plan.app_outer_class
    }))
    .ok_or(Uia2WindowResolveError::MissingAppOuter)?;

    match plan.shape {
        Uia2WindowShape::DirectOuterWindow => Ok(resolved(plan, app_outer.hwnd, app_outer)),
        Uia2WindowShape::ChromiumRendererChild => {
            let renderer_class = plan
                .renderer_child_class
                .ok_or(Uia2WindowResolveError::MissingRendererChild)?;
            let renderer = largest_visible(windows.iter().copied().filter(|window| {
                window.class_name == renderer_class
                    && is_descendant_of(window.hwnd, app_outer.hwnd, windows)
            }))
            .ok_or(Uia2WindowResolveError::MissingRendererChild)?;
            Ok(resolved(plan, app_outer.hwnd, renderer))
        }
        Uia2WindowShape::SiblingChromiumRenderer => {
            let sibling_process = plan
                .sibling_process_name
                .ok_or(Uia2WindowResolveError::MissingSiblingContentOuter)?;
            let sibling_outer_class = plan
                .sibling_outer_class
                .ok_or(Uia2WindowResolveError::MissingSiblingContentOuter)?;
            let renderer_class = plan
                .renderer_child_class
                .ok_or(Uia2WindowResolveError::MissingRendererChild)?;

            // Two WebView2 hosts run side by side on the owner's machine and the
            // one that is NOT WhatsApp's has the larger window, so the shape of
            // this loop matters: every visible host of the right image and class
            // is classified first, and nothing is chosen by size at all.
            //
            // The `visible` filter is part of the decision, not an optimisation.
            // Without it a single invisible zero-area sibling with an unreadable
            // command line -- a crashpad watcher, an IME window, a `Chrome_-
            // WidgetWin_0` shim, all of which the live enumeration returns --
            // classifies `Uncorroborated` and refuses a perfectly healthy
            // WhatsApp. A window OSL could never bind must not be able to veto
            // one it can.
            let mut owned_hosts = Vec::new();
            for host in windows.iter().copied().filter(|window| {
                same_process_name(window.process_name, sibling_process)
                    && window.class_name == sibling_outer_class
                    && window.visible
                    && window.hwnd != 0
            }) {
                match classify_sibling_host(app_outer, host) {
                    SiblingHostAssociation::OwnedByApp => owned_hosts.push(host),
                    // A host tied to this shell by one signal while the other
                    // names a different application is a state nobody has
                    // measured. It must not silently resolve to a guess, and it
                    // must not be skipped over in favour of some other window
                    // either -- the whole graph is suspect at that point. This
                    // branch is consequential precisely when a legitimate host
                    // is ALSO present, which is the case its test must carry.
                    SiblingHostAssociation::Contradicted
                    | SiblingHostAssociation::Uncorroborated => {
                        return Err(Uia2WindowResolveError::MissingSiblingContentOuter)
                    }
                    SiblingHostAssociation::Foreign => {}
                }
            }

            // AMBIGUITY FAILS CLOSED. The association is an anchor, not a unique
            // key: nothing stops two hosts satisfying both signals at once, and
            // the first version of this arm handed that case to
            // `largest_visible` -- mutant [1]'s rule, merely restricted to the
            // owned set, and it bound the wrong window against unmutated code.
            //
            // There is no second line of defence downstream: the placement path
            // checks only that the bound window is not the app shell, so a wrong
            // bind here places the payload's carrier into a window the user
            // never chose. Size is not evidence of which host holds the
            // conversation. If a legitimate second window ever appears -- a
            // popped-out chat, a media viewer -- the answer is a POSITIVE
            // discriminator, the composer matcher deciding which host actually
            // holds the conversation UI. It is never a size heuristic.
            let content_outer = match owned_hosts.as_slice() {
                [single] => *single,
                _ => return Err(Uia2WindowResolveError::MissingSiblingContentOuter),
            };
            let renderer = largest_visible(windows.iter().copied().filter(|window| {
                window.class_name == renderer_class
                    && is_descendant_of(window.hwnd, content_outer.hwnd, windows)
            }))
            .ok_or(Uia2WindowResolveError::MissingRendererChild)?;
            Ok(resolved(plan, app_outer.hwnd, renderer))
        }
    }
}

/// The command-line switch a WebView2 host carries naming the application whose
/// content it is hosting.
///
/// Measured on the owner's host: WhatsApp's `msedgewebview2` carries
/// `--webview-exe-name=WhatsApp.Root.exe`, and Windows Search's carries
/// `--webview-exe-name=SearchApp.exe`. The two processes are otherwise
/// indistinguishable by image name, which is why matching on the name alone
/// cannot tell them apart.
pub const WEBVIEW2_HOST_EXE_SWITCH: &str = "--webview-exe-name=";

/// The value of [`WEBVIEW2_HOST_EXE_SWITCH`] in a WebView2 host's command line.
///
/// Portable on purpose. Reading another process' command line is `cfg(windows)`
/// and therefore neither compiled nor tested by a Linux build; deciding what the
/// switch says is a decision over a string, so it is one, and the measured
/// command lines from the owner's host are unit tests below.
pub fn webview2_host_exe_name(command_line: &str) -> Option<&str> {
    let value = command_line.split(WEBVIEW2_HOST_EXE_SWITCH).nth(1)?;
    let value = if let Some(quoted) = value.strip_prefix('"') {
        quoted.split('"').next().unwrap_or_default()
    } else {
        value
            .split(|character: char| character.is_whitespace())
            .next()
            .unwrap_or_default()
    };
    (!value.is_empty()).then_some(value)
}

/// How a candidate WebView2 host window relates to the app shell OSL resolved.
///
/// The distinction that earns its keep is [`Self::Foreign`] versus the two
/// refusing variants. A foreign host is another application's and is simply not
/// this app's content window -- Windows Search's, on the owner's machine, and it
/// is the bigger of the two. The refusing variants are a host that IS tied to
/// this app by one signal and not confirmed by the other, which has never been
/// observed and is not something to resolve by guessing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SiblingHostAssociation {
    /// This app's content host. Either the window is rooted at the shell's own
    /// handle, or the host process is the shell's child **and** declares it is
    /// hosting the shell's image.
    OwnedByApp,
    /// The host process is the shell's child, or its window is rooted at the
    /// shell, but its `--webview-exe-name` names a different application.
    Contradicted,
    /// The host process is the shell's child but nothing corroborates it: no
    /// `--webview-exe-name` could be read. Parentage alone is not enough to
    /// place a carrier on, so this refuses too.
    Uncorroborated,
    /// Another application's WebView2 host. Not an error -- just not ours.
    Foreign,
}

/// Decide whether one WebView2 host window belongs to the app shell OSL claimed.
///
/// # Why parentage and the switch are corroborating, not alternative
///
/// `ParentProcessId` is the link: it is the only relationship WhatsApp's shell
/// and its WebView2 actually have on the owner's host, and it is the one the
/// conductor re-measured (24196 -> 23884). But a pid can be reused after a
/// process exits, and "a child process of the app" is a broader claim than "the
/// app's content window". `--webview-exe-name` is the host's own statement of
/// which application it is hosting, so requiring both means an *accidental*
/// wrong answer needs a reused pid **and** a matching declaration.
///
/// # What this is NOT: it is not a defence against a spoofing adversary
///
/// State this plainly, because the conjunction above reads like a security
/// control and is not one. A process can choose its apparent parent --
/// `CreateProcess` with `PROC_THREAD_ATTRIBUTE_PARENT_PROCESS` sets
/// `InheritedFromUniqueProcessId` to whatever pid the caller names -- and a
/// command line is chosen by whoever launches the process. **Both signals are
/// attacker-controlled, so requiring both costs an adversary nothing.**
///
/// What this is, and what it is worth: an **accident** control. It is what
/// separates Windows Search's WebView2 from WhatsApp's on a machine running
/// both, which is the failure that was actually measured and would actually
/// have happened. Anything stronger -- code identity, package identity, a
/// signature check on the hosting image -- is a different mechanism and is not
/// claimed here.
///
/// Window ancestry is accepted on its own because it is not an inference: a
/// window whose root ancestor is the shell's handle is inside the shell's
/// window, decided by the window manager rather than read out of a process
/// table. It is kept for providers that do embed their content that way; it is
/// **not** the link WhatsApp has, which is the whole of D-156.
pub fn classify_sibling_host(
    app_outer: Uia2WindowCandidate<'_>,
    host: Uia2WindowCandidate<'_>,
) -> SiblingHostAssociation {
    let declares = host
        .host_exe_name
        .map(|name| same_image_name(name, app_outer.process_name));
    let parented = host.parent_process_id != 0 && host.parent_process_id == app_outer.process_id;
    let rooted = host.associated_app_hwnd == Some(app_outer.hwnd);

    match (parented, rooted, declares) {
        (_, _, Some(false)) if parented || rooted => SiblingHostAssociation::Contradicted,
        (true, _, Some(true)) => SiblingHostAssociation::OwnedByApp,
        (true, _, None) => SiblingHostAssociation::Uncorroborated,
        (false, true, _) => SiblingHostAssociation::OwnedByApp,
        _ => SiblingHostAssociation::Foreign,
    }
}

fn resolved(
    plan: Uia2WindowPlan,
    app_outer_hwnd: isize,
    bound: Uia2WindowCandidate<'_>,
) -> Uia2ResolvedWindow {
    Uia2ResolvedWindow {
        app_outer_hwnd,
        bound_hwnd: bound.hwnd,
        bound_process_id: bound.process_id,
        wake_policy: plan.wake_policy,
        tree_route: plan.tree_route,
        poll_until_populated: plan.poll_until_populated,
        populated_min_elements: plan.populated_min_elements,
        call_timeout_ms: plan.call_timeout_ms,
    }
}

fn largest_visible<'a>(
    windows: impl Iterator<Item = Uia2WindowCandidate<'a>>,
) -> Option<Uia2WindowCandidate<'a>> {
    windows
        .filter(|window| window.visible && window.hwnd != 0)
        .max_by_key(|window| window.area)
}

fn is_descendant_of(
    child_hwnd: isize,
    ancestor_hwnd: isize,
    windows: &[Uia2WindowCandidate<'_>],
) -> bool {
    let mut current = windows
        .iter()
        .find(|window| window.hwnd == child_hwnd)
        .and_then(|window| window.parent_hwnd);
    while let Some(hwnd) = current {
        if hwnd == ancestor_hwnd {
            return true;
        }
        current = windows
            .iter()
            .find(|window| window.hwnd == hwnd)
            .and_then(|window| window.parent_hwnd);
    }
    false
}

fn same_process_name(actual: &str, expected: &str) -> bool {
    let actual = actual.strip_suffix(".exe").unwrap_or(actual);
    actual.eq_ignore_ascii_case(expected)
}

/// Compare two image names when **neither** side is a plan constant.
///
/// [`same_process_name`] strips `.exe` from the observed name only, because the
/// other side is always a plan's `app_process_name`, which never carries the
/// extension. The corroboration in [`classify_sibling_host`] compares two
/// *observed* strings -- `--webview-exe-name=WhatsApp.Root.exe` against the
/// shell window's own image, also `WhatsApp.Root.exe` -- so it has to strip
/// both. Getting this wrong classified WhatsApp's own WebView2 as
/// `Contradicted`, which is a refusal, not a mis-bind; it is written down here
/// because the failure mode was invisible in the name.
fn same_image_name(left: &str, right: &str) -> bool {
    let strip = |name: &str| {
        let lowered = name.to_ascii_lowercase();
        lowered
            .strip_suffix(".exe")
            .map(str::to_owned)
            .unwrap_or(lowered)
    };
    strip(left) == strip(right)
}

/// Execute Chromium's two-part accessibility activation handshake.
///
/// Kept generic so the ordering and object-id invariant are unit-testable on
/// non-Windows hosts.  The returned value is the owned accessibility reference
/// obtained by the second half of the handshake.
pub(crate) fn wake_electron_accessibility_with<T>(
    notify_alert: impl FnOnce(u32, i32),
    get_object: impl FnOnce(i32) -> Option<T>,
) -> Option<T> {
    notify_alert(EVENT_SYSTEM_ALERT, ELECTRON_A11Y_OBJECT_ID);
    get_object(ELECTRON_A11Y_OBJECT_ID)
}

/// The class of an `ElementFromIAccessible` HRESULT.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MsaaBridgeCallClass {
    Apartment,
    ProviderGone,
    Busy,
    Refused,
    AccessDenied,
    Resources,
    Unclassified,
}

/// Classify one `ElementFromIAccessible` HRESULT without requiring Windows
/// headers, so every native adapter shares the same retry boundary.
pub(crate) fn msaa_bridge_call_class(hresult: i32) -> MsaaBridgeCallClass {
    const CO_E_NOTINITIALIZED: i32 = 0x8004_01F0u32 as i32;
    const RPC_E_CHANGED_MODE: i32 = 0x8001_0106u32 as i32;
    const RPC_E_WRONG_THREAD: i32 = 0x8001_010Eu32 as i32;
    const RPC_E_THREAD_NOT_INIT: i32 = 0x8001_010Fu32 as i32;
    const RPC_E_DISCONNECTED: i32 = 0x8001_0108u32 as i32;
    const RPC_E_SERVERFAULT: i32 = 0x8001_0105u32 as i32;
    const RPC_S_SERVER_UNAVAILABLE: i32 = 0x8007_06BAu32 as i32;
    const RPC_S_CALL_FAILED: i32 = 0x8007_06BEu32 as i32;
    const UIA_E_ELEMENTNOTAVAILABLE: i32 = 0x8004_0201u32 as i32;
    const RPC_E_CALL_REJECTED: i32 = 0x8001_0001u32 as i32;
    const RPC_E_SERVERCALL_RETRYLATER: i32 = 0x8001_010Au32 as i32;
    const RPC_E_TIMEOUT: i32 = 0x8001_011Fu32 as i32;
    const UIA_E_TIMEOUT: i32 = 0x8013_1505u32 as i32;
    const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
    const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
    const E_POINTER: i32 = 0x8000_4003u32 as i32;
    const E_FAIL: i32 = 0x8000_4005u32 as i32;
    const UIA_E_ELEMENTNOTENABLED: i32 = 0x8004_0200u32 as i32;
    const UIA_E_NOTSUPPORTED: i32 = 0x8004_0204u32 as i32;
    const E_ACCESSDENIED: i32 = 0x8007_0005u32 as i32;
    const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;

    match hresult {
        CO_E_NOTINITIALIZED | RPC_E_CHANGED_MODE | RPC_E_WRONG_THREAD | RPC_E_THREAD_NOT_INIT => {
            MsaaBridgeCallClass::Apartment
        }
        RPC_E_DISCONNECTED
        | RPC_E_SERVERFAULT
        | RPC_S_SERVER_UNAVAILABLE
        | RPC_S_CALL_FAILED
        | UIA_E_ELEMENTNOTAVAILABLE => MsaaBridgeCallClass::ProviderGone,
        RPC_E_CALL_REJECTED | RPC_E_SERVERCALL_RETRYLATER | RPC_E_TIMEOUT | UIA_E_TIMEOUT => {
            MsaaBridgeCallClass::Busy
        }
        E_INVALIDARG
        | E_NOINTERFACE
        | E_POINTER
        | E_FAIL
        | UIA_E_ELEMENTNOTENABLED
        | UIA_E_NOTSUPPORTED => MsaaBridgeCallClass::Refused,
        E_ACCESSDENIED => MsaaBridgeCallClass::AccessDenied,
        E_OUTOFMEMORY => MsaaBridgeCallClass::Resources,
        _ => MsaaBridgeCallClass::Unclassified,
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn wake_electron_accessibility(
    window: isize,
) -> Option<::windows::Win32::UI::Accessibility::IAccessible> {
    use std::ffi::c_void;
    use windows::core::Interface;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Accessibility::{
        AccessibleObjectFromWindow, IAccessible, NotifyWinEvent,
    };

    if window == 0 {
        return None;
    }
    wake_electron_accessibility_with(
        |event, object_id| unsafe { NotifyWinEvent(event, HWND(window as _), object_id, 0) },
        |object_id| {
            let mut object: *mut c_void = std::ptr::null_mut();
            unsafe {
                AccessibleObjectFromWindow(
                    HWND(window as _),
                    object_id as u32,
                    &IAccessible::IID,
                    &mut object,
                )
            }
            .ok()?;
            (!object.is_null()).then(|| unsafe { IAccessible::from_raw(object) })
        },
    )
}

/// Bridge one MSAA object into UI Automation. This is intentionally only the
/// provider call: each service adapter retains responsibility for validating
/// process ownership, visibility, and geometry of the returned element.
#[cfg(target_os = "windows")]
pub(crate) fn element_from_ia_accessible(
    automation: &::windows::Win32::UI::Accessibility::IUIAutomation,
    accessible: &::windows::Win32::UI::Accessibility::IAccessible,
) -> Result<::windows::Win32::UI::Accessibility::IUIAutomationElement, MsaaBridgeCallClass> {
    unsafe { automation.ElementFromIAccessible(accessible, 0) }
        .map_err(|error| msaa_bridge_call_class(error.code().0))
}

// ---------------------------------------------------------------------------
// The producer
//
// Everything above this line is a decision over data. Everything below is the
// pipeline that feeds it real windows: enumerate -> resolve the shape -> wake
// per `wake_policy` -> poll until the tree populates -> resolve the composer ->
// place through the bounded `ValuePattern` call.
//
// The pipeline itself is portable. Only `Uia2Syscalls` touches Windows, and the
// only implementor that does is `win32`, below, behind `cfg(target_os =
// "windows")`. That split is deliberate: `cfg(windows)` code is neither
// compiled nor tested by a plain Linux build, so anything that can be a
// decision over data is one, and the tests drive the whole pipeline with
// recorded window graphs from A-00's four measured providers.
// ---------------------------------------------------------------------------

/// The deadline one cross-process accessibility call must answer inside.
///
/// It is a token, not a number the caller picks: the only constructor is
/// private to this module, so a `Uia2Syscalls` method can only be reached with
/// a deadline the acquisition derived from `Uia2WindowPlan::call_timeout_ms`.
/// That is what stops `call_timeout_ms` from being a declaration again.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Uia2Deadline(u64);

impl Uia2Deadline {
    fn from_plan(plan: Uia2WindowPlan) -> Self {
        Self(plan.call_timeout_ms)
    }

    pub fn millis(self) -> u64 {
        self.0
    }
}

/// One enumerated window, owned, so it can cross the syscall seam.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Uia2OwnedWindow {
    pub hwnd: isize,
    pub parent_hwnd: Option<isize>,
    pub associated_app_hwnd: Option<isize>,
    pub process_id: u32,
    pub process_name: String,
    /// See [`Uia2WindowCandidate::parent_process_id`]. `0` is "not read".
    pub parent_process_id: u32,
    /// See [`Uia2WindowCandidate::host_exe_name`].
    pub host_exe_name: Option<String>,
    pub class_name: String,
    pub visible: bool,
    pub area: u32,
}

impl Uia2OwnedWindow {
    pub fn candidate(&self) -> Uia2WindowCandidate<'_> {
        Uia2WindowCandidate {
            hwnd: self.hwnd,
            parent_hwnd: self.parent_hwnd,
            associated_app_hwnd: self.associated_app_hwnd,
            process_id: self.process_id,
            process_name: &self.process_name,
            parent_process_id: self.parent_process_id,
            host_exe_name: self.host_exe_name.as_deref(),
            class_name: &self.class_name,
            visible: self.visible,
            area: self.area,
        }
    }

    /// Record the process-table facts this window's association depends on.
    ///
    /// Kept as a builder so every existing construction site -- including the
    /// three adapters' recorded graphs, which this task must leave byte-for-byte
    /// unchanged in behaviour -- keeps compiling with the fields absent, which
    /// is exactly what "the parentage was never read" should mean.
    pub fn hosted_by(mut self, parent_process_id: u32, host_exe_name: Option<&str>) -> Self {
        self.parent_process_id = parent_process_id;
        self.host_exe_name = host_exe_name.map(str::to_owned);
        self
    }
}

/// One editable element, reduced to the facts a composer decision needs.
///
/// A-00's measured rule, in fields: an element is only a candidate if it
/// exposes `ValuePattern`, is enabled and is keyboard focusable. `read_only`
/// is carried separately because a `ValuePattern` that reports itself
/// read-only cannot be written and must not be counted as a composer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Uia2Editable {
    pub runtime_id: Vec<i32>,
    pub name: String,
    pub value_pattern: bool,
    pub enabled: bool,
    pub keyboard_focusable: bool,
    pub read_only: bool,
}

impl Uia2Editable {
    pub fn writable(&self) -> bool {
        self.value_pattern && self.enabled && self.keyboard_focusable && !self.read_only
    }
}

/// The thin syscall layer, and the only part of the producer that is allowed
/// to know about Windows.
///
/// Every method carries a [`Uia2Deadline`] because every one of them is a
/// cross-process call into another application's UI thread. There is
/// deliberately no verb here that could commit a message: no key, no message
/// post, no pattern that activates a control. The placement path can write a
/// value and read it back, and that is the whole vocabulary.
pub trait Uia2Syscalls {
    fn enumerate_windows(
        &self,
        deadline: Uia2Deadline,
    ) -> Result<Vec<Uia2OwnedWindow>, Uia2CallTimeout>;

    /// Run Chromium's accessibility handshake at this window. `Ok(false)` means
    /// the handshake was refused, which is not a timeout.
    fn wake_chromium(&self, hwnd: isize, deadline: Uia2Deadline)
        -> Result<bool, Uia2CallTimeout>;

    fn element_count(
        &self,
        hwnd: isize,
        route: Uia2TreeRoute,
        deadline: Uia2Deadline,
    ) -> Result<usize, Uia2CallTimeout>;

    fn editable_elements(
        &self,
        hwnd: isize,
        route: Uia2TreeRoute,
        deadline: Uia2Deadline,
    ) -> Result<Vec<Uia2Editable>, Uia2CallTimeout>;

    fn set_value(
        &self,
        hwnd: isize,
        route: Uia2TreeRoute,
        element: &Uia2Editable,
        value: &str,
        deadline: Uia2Deadline,
    ) -> Result<bool, Uia2CallTimeout>;

    fn value_of(
        &self,
        hwnd: isize,
        route: Uia2TreeRoute,
        element: &Uia2Editable,
        deadline: Uia2Deadline,
    ) -> Result<Option<String>, Uia2CallTimeout>;

    /// How many submit-shaped interactions this backend has observed. Read
    /// around every placement so the receipt's verdict comes from the backend
    /// rather than from a literal in the caller. D-139's finding 2 was exactly
    /// a receipt field that could not be false.
    fn submit_shaped_calls(&self) -> usize;

    /// Wait for an asynchronously populated tree. Injected so the poll ladder
    /// is testable without spending its budget in real time.
    fn settle(&self, millis: u64);
}

/// The settle ladder a poll walks while waiting for Chromium to build its tree.
///
/// A-00 measured ~90 s to a fully populated Discord tree, so the ladder has to
/// reach that scale; it starts short because Telegram-shaped providers answer
/// immediately and must not be made to wait for a rung they do not need.
pub const UIA2_SETTLE_LADDER_MS: &[u64] = &[150, 300, 500, 1_000, 2_000, 5_000, 10_000];

/// The waits a poll will perform for a given budget, in order, summing to at
/// most the budget. An empty plan means "read once and decide".
pub fn uia2_settle_plan(budget_ms: u64) -> Vec<u64> {
    let mut plan = Vec::new();
    let mut spent = 0u64;
    let mut rung = 0usize;
    while spent < budget_ms {
        let step = UIA2_SETTLE_LADDER_MS[rung.min(UIA2_SETTLE_LADDER_MS.len() - 1)];
        let step = step.min(budget_ms - spent);
        if step == 0 {
            break;
        }
        spent += step;
        plan.push(step);
        rung += 1;
    }
    plan
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Uia2AcquireError {
    Resolve(Uia2WindowResolveError),
    /// Chromium was asked for its accessibility object and refused.
    WakeRefused,
    /// The tree never reached `populated_min_elements` inside the budget. This
    /// is the failure a missing wake produces, and it is why it is a distinct
    /// error rather than "no composer".
    TreeNeverPopulated {
        seen: usize,
        needed: usize,
    },
    CallTimedOut(Uia2CallTimeout),
}

impl From<Uia2CallTimeout> for Uia2AcquireError {
    fn from(timeout: Uia2CallTimeout) -> Self {
        Self::CallTimedOut(timeout)
    }
}

/// A window that has been enumerated, resolved, woken and polled, and is ready
/// to be read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Uia2Acquired {
    pub window: Uia2ResolvedWindow,
    pub woke: bool,
    pub elements: usize,
    pub settled_ms: u64,
}

/// Enumerate, then resolve which window this plan binds.
///
/// Split out from [`acquire_uia2_window`] because a caller that already owns a
/// live accessibility session -- Discord does -- needs the shape decision
/// without paying for a second wake or a second poll.
pub fn resolve_uia2_wake_target(
    plan: Uia2WindowPlan,
    host: &dyn Uia2Syscalls,
) -> Result<Uia2ResolvedWindow, Uia2AcquireError> {
    let deadline = Uia2Deadline::from_plan(plan);
    let windows = host.enumerate_windows(deadline)?;
    let candidates = windows
        .iter()
        .map(Uia2OwnedWindow::candidate)
        .collect::<Vec<_>>();
    resolve_uia2_window(plan, &candidates).map_err(Uia2AcquireError::Resolve)
}

/// The whole producer: enumerate -> resolve the shape -> wake per
/// `wake_policy` -> poll until the tree populates.
pub fn acquire_uia2_window(
    plan: Uia2WindowPlan,
    host: &dyn Uia2Syscalls,
) -> Result<Uia2Acquired, Uia2AcquireError> {
    let deadline = Uia2Deadline::from_plan(plan);
    let window = resolve_uia2_wake_target(plan, host)?;

    let woke = match window.wake_policy {
        Uia2WakePolicy::None => false,
        Uia2WakePolicy::WmGetObjectChromium => {
            if !host.wake_chromium(window.bound_hwnd, deadline)? {
                return Err(Uia2AcquireError::WakeRefused);
            }
            true
        }
    };

    let mut elements = host.element_count(window.bound_hwnd, window.tree_route, deadline)?;
    let mut settled_ms = 0u64;
    if window.poll_until_populated {
        for step in uia2_settle_plan(plan.default_wait_ms) {
            if elements >= window.populated_min_elements {
                break;
            }
            host.settle(step);
            settled_ms += step;
            elements = host.element_count(window.bound_hwnd, window.tree_route, deadline)?;
        }
    }
    if elements < window.populated_min_elements {
        return Err(Uia2AcquireError::TreeNeverPopulated {
            seen: elements,
            needed: window.populated_min_elements,
        });
    }

    Ok(Uia2Acquired {
        window,
        woke,
        elements,
        settled_ms,
    })
}

/// List the editable elements of a window this producer has already acquired.
///
/// The second step of every consumer, and the one deadline-carrying syscall
/// that had no public door: `acquire_uia2_window` covers enumerate/wake/count
/// and `place_uia2_carrier`/`clear_uia2_composer` cover set/read, so before
/// this existed no module outside `native_a11y` could reach a composer
/// candidate at all (D-155).
///
/// The budget is derived here, from the acquisition's own `call_timeout_ms`,
/// exactly as its siblings derive theirs. There is deliberately **no variant
/// that takes a caller-supplied deadline**: [`Uia2Deadline`]'s private
/// constructor is what makes an unbounded cross-process call unrepresentable,
/// and a public function that accepted one would be that hole with a different
/// name.
pub fn acquire_uia2_editables(
    host: &dyn Uia2Syscalls,
    acquired: Uia2Acquired,
) -> Result<Vec<Uia2Editable>, Uia2CallTimeout> {
    let window = acquired.window;
    let deadline = Uia2Deadline(window.call_timeout_ms);
    host.editable_elements(window.bound_hwnd, window.tree_route, deadline)
}

/// How a provider's composer is told apart from every other editable element,
/// including the search box that A-00 wrote into by accident and D-139 found
/// still admissible.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Uia2ComposerMatcher {
    /// Lowercase stems, any of which admits a name.
    pub composer_stems: &'static [&'static str],
    /// Lowercase stems, any of which rejects a name outright. Checked first,
    /// so "search messages" is refused even though it contains "message".
    pub non_composer_stems: &'static [&'static str],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Uia2ComposerError {
    /// The tree exposed no editable element at all.
    NoEditable,
    /// Editable elements exist, but none of them is writable.
    NoWritableEditable,
    /// Writable elements exist, but none of their names is a composer.
    NoComposerName,
    /// More than one writable element claims to be the composer. Refused
    /// rather than guessed at: writing into the wrong one is a disclosure.
    Ambiguous(usize),
}

fn uia2_normalized_name(name: &str) -> String {
    name.trim()
        .trim_end_matches('.')
        .replace('\u{2026}', "")
        .to_lowercase()
}

pub fn uia2_name_is_composer(matcher: Uia2ComposerMatcher, name: &str) -> bool {
    let normalized = uia2_normalized_name(name);
    if matcher
        .non_composer_stems
        .iter()
        .any(|stem| normalized.contains(stem))
    {
        return false;
    }
    matcher
        .composer_stems
        .iter()
        .any(|stem| normalized.contains(stem))
}

/// Pick the one writable element that is this provider's composer.
pub fn resolve_uia2_composer(
    matcher: Uia2ComposerMatcher,
    elements: &[Uia2Editable],
) -> Result<Uia2Editable, Uia2ComposerError> {
    if elements.is_empty() {
        return Err(Uia2ComposerError::NoEditable);
    }
    let writable = elements
        .iter()
        .filter(|element| element.writable())
        .collect::<Vec<_>>();
    if writable.is_empty() {
        return Err(Uia2ComposerError::NoWritableEditable);
    }
    let named = writable
        .into_iter()
        .filter(|element| uia2_name_is_composer(matcher, &element.name))
        .collect::<Vec<_>>();
    match named.len() {
        0 => Err(Uia2ComposerError::NoComposerName),
        1 => Ok(named[0].clone()),
        more => Err(Uia2ComposerError::Ambiguous(more)),
    }
}

/// A carrier that carries a line break is a submit, not a value.
///
/// Every provider OSL drives commits the message on Enter, so a newline
/// smuggled into the carrier *is* the send. Refused at the mechanism, before
/// anything reaches a live composer.
pub fn uia2_carrier_carries_submit(carrier: &str) -> bool {
    carrier
        .chars()
        .any(|character| matches!(character, '\n' | '\r' | '\u{000b}' | '\u{2028}' | '\u{2029}'))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Uia2PlacementReceipt {
    pub placed: bool,
    /// Read back with `contains`, never `equals`: a live UI augments its own
    /// fields, and A-00 nearly discarded the decisive Discord result to an
    /// exact-equality check.
    pub readback_holds_carrier: bool,
    /// Derived from the backend's own submit-shaped counter, never from a
    /// literal. If this is ever true the placement is refused.
    pub submit_shaped_observed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Uia2PlacementRefusal {
    EmptyCarrier,
    CarrierCarriesSubmit,
    /// The composer already holds text OSL did not put there.
    ExistingDraft,
    SetValueRefused,
    ReadbackMissingCarrier,
    /// The backend observed a submit-shaped interaction. Nothing further runs.
    SubmitShaped,
    CallTimedOut(Uia2CallTimeout),
}

impl From<Uia2CallTimeout> for Uia2PlacementRefusal {
    fn from(timeout: Uia2CallTimeout) -> Self {
        Self::CallTimedOut(timeout)
    }
}

/// Write a carrier into a resolved composer and read it back. Placement only:
/// nothing here commits, and the syscall trait exposes no verb that could.
pub fn place_uia2_carrier(
    host: &dyn Uia2Syscalls,
    acquired: Uia2Acquired,
    composer: &Uia2Editable,
    carrier: &str,
    allow_replace_existing: bool,
) -> Result<Uia2PlacementReceipt, Uia2PlacementRefusal> {
    if carrier.is_empty() {
        return Err(Uia2PlacementRefusal::EmptyCarrier);
    }
    if uia2_carrier_carries_submit(carrier) {
        return Err(Uia2PlacementRefusal::CarrierCarriesSubmit);
    }
    let window = acquired.window;
    let deadline = Uia2Deadline(window.call_timeout_ms);
    let baseline = host.submit_shaped_calls();
    if baseline > 0 {
        return Err(Uia2PlacementRefusal::SubmitShaped);
    }

    let existing = host.value_of(window.bound_hwnd, window.tree_route, composer, deadline)?;
    if !allow_replace_existing
        && existing
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
    {
        return Err(Uia2PlacementRefusal::ExistingDraft);
    }

    if !host.set_value(
        window.bound_hwnd,
        window.tree_route,
        composer,
        carrier,
        deadline,
    )? {
        return Err(Uia2PlacementRefusal::SetValueRefused);
    }

    let readback = host.value_of(window.bound_hwnd, window.tree_route, composer, deadline)?;
    let readback_holds_carrier = readback
        .as_deref()
        .is_some_and(|value| value.contains(carrier));
    let submit_shaped_observed = host.submit_shaped_calls() > baseline;
    if submit_shaped_observed {
        return Err(Uia2PlacementRefusal::SubmitShaped);
    }
    if !readback_holds_carrier {
        return Err(Uia2PlacementRefusal::ReadbackMissingCarrier);
    }

    Ok(Uia2PlacementReceipt {
        placed: true,
        readback_holds_carrier,
        submit_shaped_observed,
    })
}

/// Clear a composer OSL wrote into. Always run after a probe: never leave text
/// in a real person's chat.
pub fn clear_uia2_composer(
    host: &dyn Uia2Syscalls,
    acquired: Uia2Acquired,
    composer: &Uia2Editable,
) -> Result<(), Uia2PlacementRefusal> {
    let window = acquired.window;
    let deadline = Uia2Deadline(window.call_timeout_ms);
    let baseline = host.submit_shaped_calls();
    if !host.set_value(window.bound_hwnd, window.tree_route, composer, "", deadline)? {
        return Err(Uia2PlacementRefusal::SetValueRefused);
    }
    if host.submit_shaped_calls() > baseline {
        return Err(Uia2PlacementRefusal::SubmitShaped);
    }
    Ok(())
}

/// The live Windows implementation of the syscall seam.
///
/// Nothing in here decides anything: it enumerates, wakes, counts, reads and
/// writes, and every one of those is issued through [`call_with_timeout`] on a
/// worker thread so a provider that stops answering costs a parked thread
/// rather than a frozen OSL.
#[cfg(target_os = "windows")]
pub(crate) mod win32 {
    use super::{
        call_with_timeout, same_process_name, webview2_host_exe_name, Uia2CallTimeout, Uia2Deadline,
        Uia2Editable, Uia2OwnedWindow, Uia2Syscalls, Uia2TreeRoute, WEBVIEW2_PROCESS_NAME,
    };

    use ::windows::core::Interface;
    use ::windows::Win32::Foundation::HWND;
    use ::windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use ::windows::Win32::System::Ole::{
        SafeArrayAccessData, SafeArrayDestroy, SafeArrayGetLBound, SafeArrayGetUBound,
        SafeArrayUnaccessData,
    };
    use ::windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationValuePattern,
        TreeScope_Subtree, UIA_DocumentControlTypeId, UIA_EditControlTypeId,
        UIA_TextControlTypeId, UIA_ValuePatternId,
    };
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Which windows an enumeration is allowed to see.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) enum Uia2EnumerationScope {
        /// Every top-level window on the desktop, plus their descendants. What
        /// a provider OSL has not claimed needs.
        Desktop,
        /// One already-claimed window, plus its descendants and any
        /// out-of-process content window rooted at it. Used when the caller
        /// already owns the window: a claimed window may have been reparented
        /// out of the top-level set, so a desktop walk would not find it.
        RootedAt(isize),
    }

    pub(crate) struct Uia2Win32Host {
        scope: Uia2EnumerationScope,
        submit_shaped: AtomicUsize,
    }

    impl Uia2Win32Host {
        pub(crate) fn desktop() -> Self {
            Self {
                scope: Uia2EnumerationScope::Desktop,
                submit_shaped: AtomicUsize::new(0),
            }
        }

        pub(crate) fn rooted_at(window: isize) -> Self {
            Self {
                scope: Uia2EnumerationScope::RootedAt(window),
                submit_shaped: AtomicUsize::new(0),
            }
        }
    }

    struct ComGuard(bool);

    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    fn class_name_of(window: isize) -> String {
        use windows_sys::Win32::UI::WindowsAndMessaging::GetClassNameW;
        let mut buffer = [0u16; 256];
        let length =
            unsafe { GetClassNameW(window as _, buffer.as_mut_ptr(), buffer.len() as i32) };
        if length <= 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buffer[..length as usize])
    }

    fn process_id_of(window: isize) -> u32 {
        use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
        let mut process_id = 0u32;
        if unsafe { GetWindowThreadProcessId(window as _, &mut process_id) } == 0 {
            return 0;
        }
        process_id
    }

    fn process_name_of(process_id: u32) -> String {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        if process_id == 0 {
            return String::new();
        }
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
        if handle.is_null() {
            return String::new();
        }
        let mut buffer = [0u16; 512];
        let mut length = buffer.len() as u32;
        let ok = unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut length) };
        unsafe { CloseHandle(handle) };
        if ok == 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buffer[..length as usize])
            .rsplit('\\')
            .next()
            .unwrap_or_default()
            .to_owned()
    }

    // `NtQueryInformationProcess` and `ReadProcessMemory` are declared here
    // rather than pulled from `windows-sys` because reaching them there means
    // adding `Win32_System_Diagnostics_Debug` to the crate's feature list, and a
    // build-config edit in this repository has broken every build three separate
    // times. `mullvad_window_host.rs:402` already declares its kernel32 imports
    // the same way.
    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn NtQueryInformationProcess(
            process: *mut c_void,
            information_class: u32,
            information: *mut c_void,
            information_length: u32,
            return_length: *mut u32,
        ) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn ReadProcessMemory(
            process: *mut c_void,
            address: *const c_void,
            buffer: *mut c_void,
            size: usize,
            read: *mut usize,
        ) -> i32;
    }

    /// `PROCESS_BASIC_INFORMATION`, x64 layout. Only two fields are read:
    /// `peb_base_address`, which is the door to the command line, and
    /// `inherited_from_unique_process_id`, which is the parentage D-156 needs.
    #[repr(C)]
    #[derive(Default)]
    struct ProcessBasicInformation {
        exit_status: i32,
        _padding: i32,
        peb_base_address: usize,
        affinity_mask: usize,
        base_priority: i32,
        _padding2: i32,
        unique_process_id: usize,
        inherited_from_unique_process_id: usize,
    }

    const PROCESS_BASIC_INFORMATION_CLASS: u32 = 0;
    /// x64 `PEB::ProcessParameters`.
    const PEB_PROCESS_PARAMETERS_OFFSET: usize = 0x20;
    /// x64 `RTL_USER_PROCESS_PARAMETERS::CommandLine`.
    const PROCESS_PARAMETERS_COMMAND_LINE_OFFSET: usize = 0x70;
    /// A command line longer than this is not one this decision can use.
    const MAX_COMMAND_LINE_BYTES: usize = 64 * 1024;

    fn open_for_query(process_id: u32, extra_access: u32) -> Option<*mut c_void> {
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        if process_id == 0 {
            return None;
        }
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | extra_access,
                0,
                process_id,
            )
        };
        (!handle.is_null()).then_some(handle)
    }

    fn basic_information(handle: *mut c_void) -> Option<ProcessBasicInformation> {
        let mut information = ProcessBasicInformation::default();
        let status = unsafe {
            NtQueryInformationProcess(
                handle,
                PROCESS_BASIC_INFORMATION_CLASS,
                &mut information as *mut ProcessBasicInformation as *mut c_void,
                std::mem::size_of::<ProcessBasicInformation>() as u32,
                std::ptr::null_mut(),
            )
        };
        (status >= 0).then_some(information)
    }

    fn read_exact_at<T>(handle: *mut c_void, address: usize, out: &mut T) -> bool {
        if address == 0 {
            return false;
        }
        let size = std::mem::size_of::<T>();
        let mut read = 0usize;
        let ok = unsafe {
            ReadProcessMemory(
                handle,
                address as *const c_void,
                out as *mut T as *mut c_void,
                size,
                &mut read,
            )
        };
        ok != 0 && read == size
    }

    /// The parent process id, read from the process itself.
    ///
    /// This is the WhatsApp link: the WebView2 host is a child of the shell
    /// process. It is deliberately not corroboration on its own -- see
    /// [`super::classify_sibling_host`].
    fn parent_process_id_of(process_id: u32) -> u32 {
        use windows_sys::Win32::Foundation::CloseHandle;
        let Some(handle) = open_for_query(process_id, 0) else {
            return 0;
        };
        let information = basic_information(handle);
        unsafe { CloseHandle(handle) };
        information
            .map(|information| information.inherited_from_unique_process_id as u32)
            .unwrap_or(0)
    }

    /// This process' full command line, read out of its PEB.
    ///
    /// Returns `None` on any refusal at all. That is the safe direction: an
    /// unreadable command line means the corroboration is absent, and an absent
    /// corroboration refuses rather than resolves.
    fn command_line_of(process_id: u32) -> Option<String> {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::PROCESS_VM_READ;

        let handle = open_for_query(process_id, PROCESS_VM_READ)?;
        let command_line = (|| {
            let information = basic_information(handle)?;
            let mut parameters = 0usize;
            if !read_exact_at(
                handle,
                information
                    .peb_base_address
                    .checked_add(PEB_PROCESS_PARAMETERS_OFFSET)?,
                &mut parameters,
            ) {
                return None;
            }
            // UNICODE_STRING, x64: Length: u16, MaximumLength: u16, 4 bytes of
            // padding, Buffer: *mut u16.
            let mut length = 0u16;
            if !read_exact_at(
                handle,
                parameters.checked_add(PROCESS_PARAMETERS_COMMAND_LINE_OFFSET)?,
                &mut length,
            ) {
                return None;
            }
            let mut buffer_address = 0usize;
            if !read_exact_at(
                handle,
                parameters
                    .checked_add(PROCESS_PARAMETERS_COMMAND_LINE_OFFSET)?
                    .checked_add(8)?,
                &mut buffer_address,
            ) {
                return None;
            }
            let bytes = usize::from(length);
            if bytes == 0 || bytes > MAX_COMMAND_LINE_BYTES || bytes % 2 != 0 {
                return None;
            }
            let mut wide = vec![0u16; bytes / 2];
            let mut read = 0usize;
            let ok = unsafe {
                ReadProcessMemory(
                    handle,
                    buffer_address as *const c_void,
                    wide.as_mut_ptr() as *mut c_void,
                    bytes,
                    &mut read,
                )
            };
            (ok != 0 && read == bytes).then(|| String::from_utf16_lossy(&wide))
        })();
        unsafe { CloseHandle(handle) };
        command_line
    }

    fn visible_and_area(window: isize) -> (bool, u32) {
        use windows_sys::Win32::Foundation::RECT;
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsWindowVisible};
        let visible = unsafe { IsWindowVisible(window as _) } != 0;
        let mut rect: RECT = unsafe { std::mem::zeroed() };
        if unsafe { GetWindowRect(window as _, &mut rect) } == 0 {
            return (visible, 0);
        }
        let width = rect.right.saturating_sub(rect.left).max(0) as u32;
        let height = rect.bottom.saturating_sub(rect.top).max(0) as u32;
        (visible, width.saturating_mul(height))
    }

    fn parent_of(window: isize) -> Option<isize> {
        use windows_sys::Win32::UI::WindowsAndMessaging::GetParent;
        let parent = unsafe { GetParent(window as _) };
        (!parent.is_null()).then(|| parent as isize)
    }

    fn root_ancestor_of(window: isize) -> Option<isize> {
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetAncestor, GA_ROOT};
        let root = unsafe { GetAncestor(window as _, GA_ROOT) };
        (!root.is_null() && root as isize != window).then(|| root as isize)
    }

    unsafe extern "system" fn collect_window(
        window: windows_sys::Win32::Foundation::HWND,
        param: windows_sys::Win32::Foundation::LPARAM,
    ) -> windows_sys::Win32::Foundation::BOOL {
        let sink = &mut *(param as *mut Vec<isize>);
        sink.push(window as isize);
        1
    }

    fn descendants_of(window: isize, sink: &mut Vec<isize>) {
        use windows_sys::Win32::UI::WindowsAndMessaging::EnumChildWindows;
        let mut children = Vec::<isize>::new();
        unsafe {
            EnumChildWindows(
                window as _,
                Some(collect_window),
                &mut children as *mut Vec<isize> as isize,
            )
        };
        sink.extend(children);
    }

    fn raw_window_handles(scope: Uia2EnumerationScope) -> Vec<isize> {
        use windows_sys::Win32::UI::WindowsAndMessaging::EnumWindows;
        let mut handles = Vec::<isize>::new();
        match scope {
            Uia2EnumerationScope::Desktop => {
                let mut tops = Vec::<isize>::new();
                unsafe {
                    EnumWindows(
                        Some(collect_window),
                        &mut tops as *mut Vec<isize> as isize,
                    )
                };
                for top in tops {
                    handles.push(top);
                    descendants_of(top, &mut handles);
                }
            }
            Uia2EnumerationScope::RootedAt(root) => {
                handles.push(root);
                descendants_of(root, &mut handles);
            }
        }
        handles.sort_unstable();
        handles.dedup();
        handles
    }

    /// What one process contributes to every window it owns. Read once per
    /// process id, because a desktop walk sees hundreds of windows across a few
    /// dozen processes and each of these facts costs an `OpenProcess`.
    #[derive(Clone)]
    struct ProcessFacts {
        name: String,
        parent_process_id: u32,
        host_exe_name: Option<String>,
    }

    fn process_facts(process_id: u32) -> ProcessFacts {
        let name = process_name_of(process_id);
        // The command line is read only for WebView2 hosts. It is the one image
        // that carries `--webview-exe-name`, and a PEB read is not something to
        // do against every process on the desktop for a field nothing would
        // consult.
        let host_exe_name = same_process_name(&name, WEBVIEW2_PROCESS_NAME)
            .then(|| command_line_of(process_id))
            .flatten()
            .and_then(|command_line| {
                webview2_host_exe_name(&command_line).map(str::to_owned)
            });
        ProcessFacts {
            name,
            parent_process_id: parent_process_id_of(process_id),
            host_exe_name,
        }
    }

    /// Turn raw handles into the graph the resolver consumes.
    ///
    /// Two independent associations are recorded, and D-156 is the difference
    /// between them. `associated_app_hwnd` is the window-tree one: a content
    /// window that lives in another process but is rooted at the app shell's
    /// window. WhatsApp does not have it -- its WebView2 window is genuinely
    /// top-level -- so `parent_process_id` and `host_exe_name` carry the
    /// process-table one, which it does have.
    fn enumerate(scope: Uia2EnumerationScope) -> Vec<Uia2OwnedWindow> {
        let mut facts_by_process = std::collections::HashMap::<u32, ProcessFacts>::new();
        raw_window_handles(scope)
            .into_iter()
            .filter_map(|hwnd| {
                let process_id = process_id_of(hwnd);
                if process_id == 0 {
                    return None;
                }
                let class_name = class_name_of(hwnd);
                if class_name.is_empty() {
                    return None;
                }
                let (visible, area) = visible_and_area(hwnd);
                let parent_hwnd = parent_of(hwnd);
                let associated_app_hwnd =
                    root_ancestor_of(hwnd).filter(|root| process_id_of(*root) != process_id);
                let facts = facts_by_process
                    .entry(process_id)
                    .or_insert_with(|| process_facts(process_id))
                    .clone();
                Some(Uia2OwnedWindow {
                    hwnd,
                    parent_hwnd,
                    associated_app_hwnd,
                    process_id,
                    process_name: facts.name,
                    parent_process_id: facts.parent_process_id,
                    host_exe_name: facts.host_exe_name,
                    class_name,
                    visible,
                    area,
                })
            })
            .collect()
    }

    fn automation() -> Option<IUIAutomation> {
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }.ok()
    }

    /// The element the plan's tree route says is the root of this window's
    /// accessibility tree.
    fn tree_root(
        automation: &IUIAutomation,
        hwnd: isize,
        route: Uia2TreeRoute,
    ) -> Option<IUIAutomationElement> {
        match route {
            Uia2TreeRoute::UiaNative => {
                unsafe { automation.ElementFromHandle(HWND(hwnd as _)) }.ok()
            }
            Uia2TreeRoute::MsaaBridge => {
                let accessible = super::wake_electron_accessibility(hwnd)?;
                super::element_from_ia_accessible(automation, &accessible).ok()
            }
        }
    }

    fn subtree(root: &IUIAutomationElement, automation: &IUIAutomation) -> Vec<IUIAutomationElement> {
        let Ok(condition) = (unsafe { automation.CreateTrueCondition() }) else {
            return Vec::new();
        };
        let Ok(found) = (unsafe { root.FindAll(TreeScope_Subtree, &condition) }) else {
            return Vec::new();
        };
        let Ok(length) = (unsafe { found.Length() }) else {
            return Vec::new();
        };
        (0..length)
            .filter_map(|index| unsafe { found.GetElement(index) }.ok())
            .collect()
    }

    fn runtime_id_of(element: &IUIAutomationElement) -> Vec<i32> {
        let Ok(array) = (unsafe { element.GetRuntimeId() }) else {
            return Vec::new();
        };
        if array.is_null() {
            return Vec::new();
        }
        let values = (|| {
            let lower = unsafe { SafeArrayGetLBound(array, 1) }.ok()?;
            let upper = unsafe { SafeArrayGetUBound(array, 1) }.ok()?;
            if upper < lower || upper - lower > 64 {
                return None;
            }
            let mut data: *mut c_void = std::ptr::null_mut();
            unsafe { SafeArrayAccessData(array, &mut data) }.ok()?;
            let length = (upper - lower + 1) as usize;
            let values = if data.is_null() {
                Vec::new()
            } else {
                unsafe { std::slice::from_raw_parts(data.cast::<i32>(), length) }.to_vec()
            };
            let _ = unsafe { SafeArrayUnaccessData(array) };
            Some(values)
        })()
        .unwrap_or_default();
        let _ = unsafe { SafeArrayDestroy(array) };
        values
    }

    fn value_pattern(element: &IUIAutomationElement) -> Option<IUIAutomationValuePattern> {
        unsafe { element.GetCurrentPattern(UIA_ValuePatternId) }
            .ok()?
            .cast::<IUIAutomationValuePattern>()
            .ok()
    }

    fn editable_of(element: &IUIAutomationElement) -> Option<Uia2Editable> {
        let control_type = unsafe { element.CurrentControlType() }.ok()?;
        if control_type != UIA_EditControlTypeId
            && control_type != UIA_DocumentControlTypeId
            && control_type != UIA_TextControlTypeId
        {
            return None;
        }
        let pattern = value_pattern(element);
        Some(Uia2Editable {
            runtime_id: runtime_id_of(element),
            name: unsafe { element.CurrentName() }
                .map(|name| name.to_string())
                .unwrap_or_default(),
            value_pattern: pattern.is_some(),
            enabled: unsafe { element.CurrentIsEnabled() }
                .map(|value| value.as_bool())
                .unwrap_or(false),
            keyboard_focusable: unsafe { element.CurrentIsKeyboardFocusable() }
                .map(|value| value.as_bool())
                .unwrap_or(false),
            read_only: pattern
                .and_then(|pattern| unsafe { pattern.CurrentIsReadOnly() }.ok())
                .map(|value| value.as_bool())
                .unwrap_or(true),
        })
    }

    fn element_by_runtime_id(
        automation: &IUIAutomation,
        hwnd: isize,
        route: Uia2TreeRoute,
        runtime_id: &[i32],
    ) -> Option<IUIAutomationElement> {
        let root = tree_root(automation, hwnd, route)?;
        subtree(&root, automation)
            .into_iter()
            .find(|element| runtime_id_of(element) == runtime_id)
    }

    /// Every cross-process call in this module goes through here: its own COM
    /// apartment on a worker thread, and the caller stops waiting at the
    /// deadline.
    fn bounded<T: Send + 'static>(
        deadline: Uia2Deadline,
        call: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, Uia2CallTimeout> {
        call_with_timeout(deadline.millis(), move || {
            let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            let _com = ComGuard(initialized.is_ok());
            call()
        })
    }

    impl Uia2Syscalls for Uia2Win32Host {
        fn enumerate_windows(
            &self,
            deadline: Uia2Deadline,
        ) -> Result<Vec<Uia2OwnedWindow>, Uia2CallTimeout> {
            let scope = self.scope;
            bounded(deadline, move || enumerate(scope))
        }

        fn wake_chromium(
            &self,
            hwnd: isize,
            deadline: Uia2Deadline,
        ) -> Result<bool, Uia2CallTimeout> {
            bounded(deadline, move || {
                super::wake_electron_accessibility(hwnd).is_some()
            })
        }

        fn element_count(
            &self,
            hwnd: isize,
            route: Uia2TreeRoute,
            deadline: Uia2Deadline,
        ) -> Result<usize, Uia2CallTimeout> {
            bounded(deadline, move || {
                let Some(automation) = automation() else {
                    return 0;
                };
                let Some(root) = tree_root(&automation, hwnd, route) else {
                    return 0;
                };
                subtree(&root, &automation).len()
            })
        }

        fn editable_elements(
            &self,
            hwnd: isize,
            route: Uia2TreeRoute,
            deadline: Uia2Deadline,
        ) -> Result<Vec<Uia2Editable>, Uia2CallTimeout> {
            bounded(deadline, move || {
                let Some(automation) = automation() else {
                    return Vec::new();
                };
                let Some(root) = tree_root(&automation, hwnd, route) else {
                    return Vec::new();
                };
                subtree(&root, &automation)
                    .iter()
                    .filter_map(editable_of)
                    .collect()
            })
        }

        fn set_value(
            &self,
            hwnd: isize,
            route: Uia2TreeRoute,
            element: &Uia2Editable,
            value: &str,
            deadline: Uia2Deadline,
        ) -> Result<bool, Uia2CallTimeout> {
            let runtime_id = element.runtime_id.clone();
            let value = value.to_owned();
            bounded(deadline, move || {
                let Some(automation) = automation() else {
                    return false;
                };
                let Some(found) =
                    element_by_runtime_id(&automation, hwnd, route, &runtime_id)
                else {
                    return false;
                };
                let Some(pattern) = value_pattern(&found) else {
                    return false;
                };
                unsafe { pattern.SetValue(&::windows::core::BSTR::from(value)) }.is_ok()
            })
        }

        fn value_of(
            &self,
            hwnd: isize,
            route: Uia2TreeRoute,
            element: &Uia2Editable,
            deadline: Uia2Deadline,
        ) -> Result<Option<String>, Uia2CallTimeout> {
            let runtime_id = element.runtime_id.clone();
            bounded(deadline, move || {
                let automation = automation()?;
                let found = element_by_runtime_id(&automation, hwnd, route, &runtime_id)?;
                let pattern = value_pattern(&found)?;
                unsafe { pattern.CurrentValue() }
                    .ok()
                    .map(|value| value.to_string())
            })
        }

        fn submit_shaped_calls(&self) -> usize {
            // Nothing in this module can commit a message, so the count is
            // structurally zero rather than assumed zero: there is no verb here
            // that could raise it, which is what the source scan in
            // `native_signal_adapter.rs` enforces.
            self.submit_shaped.load(Ordering::Relaxed)
        }

        fn settle(&self, millis: u64) {
            std::thread::sleep(std::time::Duration::from_millis(millis));
        }
    }
}

/// The recorded-provider test host lives here rather than in a private test
/// module so every adapter's own tests can drive the same fake through the
/// same seam. One fake, one pipeline: a second copy would be exactly the fork
/// this task exists to avoid.
#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const WAIT_MS: u64 = 90_000;
    pub(crate) const CALL_TIMEOUT_MS: u64 = 750;

    #[test]
    fn electron_wake_alerts_then_requests_the_same_custom_object() {
        let calls = std::cell::RefCell::new(Vec::new());
        let reference = wake_electron_accessibility_with(
            |event, object_id| calls.borrow_mut().push(("alert", event as i32, object_id)),
            |object_id| {
                calls.borrow_mut().push(("get_object", 0, object_id));
                Some("owned reference")
            },
        );

        assert_eq!(reference, Some("owned reference"));
        assert_eq!(
            calls.into_inner(),
            vec![
                ("alert", EVENT_SYSTEM_ALERT as i32, ELECTRON_A11Y_OBJECT_ID),
                ("get_object", 0, ELECTRON_A11Y_OBJECT_ID),
            ]
        );
    }

    #[test]
    fn bridge_hresult_classes_keep_retryable_provider_busy_distinct() {
        assert_eq!(
            msaa_bridge_call_class(0x8001_010Au32 as i32),
            MsaaBridgeCallClass::Busy
        );
        assert_eq!(
            msaa_bridge_call_class(0x8004_0201u32 as i32),
            MsaaBridgeCallClass::ProviderGone
        );
    }

    fn window(
        hwnd: isize,
        parent_hwnd: Option<isize>,
        associated_app_hwnd: Option<isize>,
        process_id: u32,
        process_name: &'static str,
        class_name: &'static str,
        area: u32,
    ) -> Uia2WindowCandidate<'static> {
        Uia2WindowCandidate {
            hwnd,
            parent_hwnd,
            associated_app_hwnd,
            process_id,
            process_name,
            parent_process_id: 0,
            host_exe_name: None,
            class_name,
            visible: true,
            area,
        }
    }

    /// A window whose process-table facts were read: the parent process id and,
    /// for a WebView2 host, the application it declares it is hosting.
    fn hosted_window(
        hwnd: isize,
        parent_hwnd: Option<isize>,
        process_id: u32,
        parent_process_id: u32,
        host_exe_name: Option<&'static str>,
        process_name: &'static str,
        class_name: &'static str,
        area: u32,
    ) -> Uia2WindowCandidate<'static> {
        Uia2WindowCandidate {
            hwnd,
            parent_hwnd,
            associated_app_hwnd: None,
            process_id,
            process_name,
            parent_process_id,
            host_exe_name,
            class_name,
            visible: true,
            area,
        }
    }

    #[test]
    fn chromium_shape_binds_renderer_wakes_and_polls() {
        let plan =
            Uia2WindowPlan::chromium_renderer_child("Signal", "Signal", WAIT_MS, CALL_TIMEOUT_MS);
        let windows = [
            window(
                10,
                None,
                None,
                100,
                "Signal.exe",
                ELECTRON_OUTER_WINDOW_CLASS,
                900,
            ),
            window(11, Some(10), None, 100, "Signal.exe", "Intermediate", 800),
            window(
                12,
                Some(11),
                None,
                100,
                "Signal.exe",
                ELECTRON_RENDERER_WINDOW_CLASS,
                700,
            ),
        ];

        let resolved = resolve_uia2_window(plan, &windows).expect("Signal renderer should resolve");
        assert_eq!(resolved.app_outer_hwnd, 10);
        assert_eq!(resolved.bound_hwnd, 12);
        assert_eq!(resolved.wake_policy, Uia2WakePolicy::WmGetObjectChromium);
        assert!(resolved.poll_until_populated);
        assert_eq!(
            resolved.populated_min_elements,
            ELECTRON_UIA2_POPULATED_MIN_ELEMENTS
        );
        assert_eq!(resolved.call_timeout_ms, CALL_TIMEOUT_MS);
    }

    #[test]
    fn chromium_shape_requires_renderer_child() {
        let plan =
            Uia2WindowPlan::chromium_renderer_child("Signal", "Signal", WAIT_MS, CALL_TIMEOUT_MS);
        let windows = [window(
            10,
            None,
            None,
            100,
            "Signal",
            ELECTRON_OUTER_WINDOW_CLASS,
            900,
        )];

        assert_eq!(
            resolve_uia2_window(plan, &windows),
            Err(Uia2WindowResolveError::MissingRendererChild)
        );
    }

    #[test]
    fn direct_outer_shape_does_not_require_renderer_or_wake() {
        let plan = Uia2WindowPlan::direct_outer_window(
            "Telegram",
            "Telegram",
            TELEGRAM_OUTER_WINDOW_CLASS,
            CALL_TIMEOUT_MS,
        );
        let windows = [window(
            20,
            None,
            None,
            200,
            "Telegram",
            TELEGRAM_OUTER_WINDOW_CLASS,
            900,
        )];

        let resolved = resolve_uia2_window(plan, &windows).expect("Telegram outer should resolve");
        assert_eq!(resolved.bound_hwnd, 20);
        assert_eq!(resolved.wake_policy, Uia2WakePolicy::None);
        assert!(!resolved.poll_until_populated);
    }

    #[test]
    fn sibling_chromium_shape_binds_webview2_renderer_not_app_root() {
        let plan = Uia2WindowPlan::sibling_chromium_renderer(
            "WhatsApp",
            "WhatsApp",
            WHATSAPP_OUTER_WINDOW_CLASS,
            WEBVIEW2_PROCESS_NAME,
            WAIT_MS,
            CALL_TIMEOUT_MS,
        );
        let windows = [
            window(
                30,
                None,
                None,
                300,
                "WhatsApp",
                WHATSAPP_OUTER_WINDOW_CLASS,
                900,
            ),
            window(
                31,
                Some(30),
                None,
                300,
                "WhatsApp",
                "WinUIChildThatNeverPopulates",
                800,
            ),
            window(
                40,
                None,
                Some(30),
                400,
                WEBVIEW2_PROCESS_NAME,
                ELECTRON_OUTER_WINDOW_CLASS,
                850,
            ),
            window(
                41,
                Some(40),
                Some(30),
                400,
                WEBVIEW2_PROCESS_NAME,
                ELECTRON_RENDERER_WINDOW_CLASS,
                840,
            ),
        ];

        let resolved =
            resolve_uia2_window(plan, &windows).expect("WhatsApp WebView2 renderer should resolve");
        assert_eq!(resolved.app_outer_hwnd, 30);
        assert_eq!(resolved.bound_hwnd, 41);
        assert_eq!(resolved.bound_process_id, 400);
        assert_eq!(resolved.wake_policy, Uia2WakePolicy::WmGetObjectChromium);
        assert!(resolved.poll_until_populated);
    }

    fn slow_provider() -> &'static str {
        std::thread::sleep(std::time::Duration::from_millis(400));
        "the provider finally answered"
    }

    #[test]
    fn bounded_call_returns_instead_of_hanging_when_the_provider_never_answers() {
        let plan = Uia2WindowPlan::chromium_renderer_child("Signal", "Signal", WAIT_MS, 40);
        let windows = [
            window(
                10,
                None,
                None,
                100,
                "Signal.exe",
                ELECTRON_OUTER_WINDOW_CLASS,
                900,
            ),
            window(
                11,
                Some(10),
                None,
                100,
                "Signal.exe",
                ELECTRON_RENDERER_WINDOW_CLASS,
                700,
            ),
        ];
        let resolved = resolve_uia2_window(plan, &windows).expect("Signal renderer should resolve");
        assert_eq!(resolved.call_timeout_ms, 40);

        let started = std::time::Instant::now();
        let outcome = resolved.bounded_call(|| {
            // A provider that never answers, exactly like the freezes measured
            // on this machine.
            std::thread::sleep(std::time::Duration::from_secs(5));
            "unreachable"
        });

        assert_eq!(outcome, Err(Uia2CallTimeout { timeout_ms: 40 }));
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "a bounded call must return at its deadline, not wait for the provider"
        );
        assert_eq!(resolved.bounded_call(|| 41), Ok(41));
    }

    #[test]
    fn call_timeout_ms_is_the_deadline_and_not_a_declaration() {
        assert_eq!(
            call_with_timeout(30, slow_provider),
            Err(Uia2CallTimeout { timeout_ms: 30 })
        );
        assert_eq!(
            call_with_timeout(10_000, slow_provider),
            Ok("the provider finally answered")
        );
    }

    // -----------------------------------------------------------------
    // The producer, driven off Windows against A-00's recorded graphs.
    // -----------------------------------------------------------------

    pub(crate) fn owned(
        hwnd: isize,
        parent_hwnd: Option<isize>,
        associated_app_hwnd: Option<isize>,
        process_id: u32,
        process_name: &str,
        class_name: &str,
        area: u32,
    ) -> Uia2OwnedWindow {
        Uia2OwnedWindow {
            hwnd,
            parent_hwnd,
            associated_app_hwnd,
            process_id,
            process_name: process_name.to_owned(),
            // Absent, not zeroed-for-convenience: these graphs were recorded
            // before D-156 read the process table, and a fixture must not claim
            // a fact its measurement never took. `Uia2OwnedWindow::hosted_by`
            // is how a graph that DID read them says so.
            parent_process_id: 0,
            host_exe_name: None,
            class_name: class_name.to_owned(),
            visible: true,
            area,
        }
    }

    /// A recorded provider: the window graph A-00 measured, plus the one
    /// behaviour that matters for the poll -- how many elements the tree
    /// exposes before it is woken, and how many after.
    pub(crate) struct RecordedHost {
        pub(crate) windows: Vec<Uia2OwnedWindow>,
        /// What `element_count` answers before the wake, or for a provider
        /// that needs none, before the tree has settled.
        pub(crate) unpopulated_elements: usize,
        pub(crate) populated_elements: usize,
        /// How many settles the tree needs after the wake before it populates.
        pub(crate) settles_before_populated: usize,
        pub(crate) needs_wake_to_populate: bool,
        pub(crate) wake_answers: bool,
        pub(crate) editables: Vec<Uia2Editable>,
        pub(crate) value: std::cell::RefCell<Option<String>>,
        pub(crate) readback_suffix: &'static str,
        pub(crate) woken: std::cell::Cell<bool>,
        pub(crate) settles: std::cell::RefCell<Vec<u64>>,
        pub(crate) deadlines: std::cell::RefCell<Vec<u64>>,
        pub(crate) set_values: std::cell::RefCell<Vec<String>>,
        pub(crate) submit_shaped: std::cell::Cell<usize>,
        pub(crate) submit_shaped_on_set: bool,
        pub(crate) never_answers: bool,
    }

    impl RecordedHost {
        pub(crate) fn new(windows: Vec<Uia2OwnedWindow>, populated_elements: usize) -> Self {
            Self {
                windows,
                unpopulated_elements: 1,
                populated_elements,
                settles_before_populated: 0,
                needs_wake_to_populate: false,
                wake_answers: true,
                editables: Vec::new(),
                value: std::cell::RefCell::new(None),
                readback_suffix: "",
                woken: std::cell::Cell::new(false),
                settles: std::cell::RefCell::new(Vec::new()),
                deadlines: std::cell::RefCell::new(Vec::new()),
                set_values: std::cell::RefCell::new(Vec::new()),
                submit_shaped: std::cell::Cell::new(0),
                submit_shaped_on_set: false,
                never_answers: false,
            }
        }

        pub(crate) fn chromium(mut self, settles_before_populated: usize) -> Self {
            self.needs_wake_to_populate = true;
            self.settles_before_populated = settles_before_populated;
            self
        }

        pub(crate) fn with_editables(mut self, editables: Vec<Uia2Editable>) -> Self {
            self.editables = editables;
            self
        }

        fn record(&self, deadline: Uia2Deadline) -> Result<(), Uia2CallTimeout> {
            self.deadlines.borrow_mut().push(deadline.millis());
            if self.never_answers {
                return Err(Uia2CallTimeout {
                    timeout_ms: deadline.millis(),
                });
            }
            Ok(())
        }
    }

    impl Uia2Syscalls for RecordedHost {
        fn enumerate_windows(
            &self,
            deadline: Uia2Deadline,
        ) -> Result<Vec<Uia2OwnedWindow>, Uia2CallTimeout> {
            self.record(deadline)?;
            Ok(self.windows.clone())
        }

        fn wake_chromium(
            &self,
            _hwnd: isize,
            deadline: Uia2Deadline,
        ) -> Result<bool, Uia2CallTimeout> {
            self.record(deadline)?;
            self.woken.set(true);
            Ok(self.wake_answers)
        }

        fn element_count(
            &self,
            _hwnd: isize,
            _route: Uia2TreeRoute,
            deadline: Uia2Deadline,
        ) -> Result<usize, Uia2CallTimeout> {
            self.record(deadline)?;
            if self.needs_wake_to_populate && !self.woken.get() {
                return Ok(self.unpopulated_elements);
            }
            if self.settles.borrow().len() < self.settles_before_populated {
                return Ok(self.unpopulated_elements);
            }
            Ok(self.populated_elements)
        }

        fn editable_elements(
            &self,
            _hwnd: isize,
            _route: Uia2TreeRoute,
            deadline: Uia2Deadline,
        ) -> Result<Vec<Uia2Editable>, Uia2CallTimeout> {
            self.record(deadline)?;
            Ok(self.editables.clone())
        }

        fn set_value(
            &self,
            _hwnd: isize,
            _route: Uia2TreeRoute,
            _element: &Uia2Editable,
            value: &str,
            deadline: Uia2Deadline,
        ) -> Result<bool, Uia2CallTimeout> {
            self.record(deadline)?;
            self.set_values.borrow_mut().push(value.to_owned());
            if self.submit_shaped_on_set {
                self.submit_shaped.set(self.submit_shaped.get() + 1);
            }
            *self.value.borrow_mut() = (!value.is_empty())
                .then(|| format!("{value}{}", self.readback_suffix));
            Ok(true)
        }

        fn value_of(
            &self,
            _hwnd: isize,
            _route: Uia2TreeRoute,
            _element: &Uia2Editable,
            deadline: Uia2Deadline,
        ) -> Result<Option<String>, Uia2CallTimeout> {
            self.record(deadline)?;
            Ok(self.value.borrow().clone())
        }

        fn submit_shaped_calls(&self) -> usize {
            self.submit_shaped.get()
        }

        fn settle(&self, millis: u64) {
            self.settles.borrow_mut().push(millis);
        }
    }

    pub(crate) fn composer(name: &str) -> Uia2Editable {
        Uia2Editable {
            runtime_id: vec![42, 7],
            name: name.to_owned(),
            value_pattern: true,
            enabled: true,
            keyboard_focusable: true,
            read_only: false,
        }
    }

    const MATCHER: Uia2ComposerMatcher = Uia2ComposerMatcher {
        composer_stems: &["message", "nachricht", "mensaje"],
        non_composer_stems: &["search", "filter", "buscar"],
    };

    /// Discord: outer `Chrome_WidgetWin_1`, woken, read through the MSAA
    /// client object. 696 elements measured.
    pub(crate) fn discord_graph() -> Vec<Uia2OwnedWindow> {
        vec![
            owned(
                0x1001,
                None,
                None,
                4100,
                "Discord.exe",
                ELECTRON_OUTER_WINDOW_CLASS,
                1_920 * 1_040,
            ),
            owned(
                0x1002,
                Some(0x1001),
                None,
                4100,
                "Discord.exe",
                "Intermediate D3D Window",
                1_920 * 1_000,
            ),
        ]
    }

    /// Telegram: Qt, outer window directly, no renderer child, no wake.
    pub(crate) fn telegram_graph() -> Vec<Uia2OwnedWindow> {
        vec![owned(
            0x2001,
            None,
            None,
            5200,
            "Telegram.exe",
            TELEGRAM_OUTER_WINDOW_CLASS,
            1_200 * 800,
        )]
    }

    /// Signal: Electron, outer -> renderer child, woken.
    pub(crate) fn signal_graph() -> Vec<Uia2OwnedWindow> {
        vec![
            owned(
                0x3001,
                None,
                None,
                6300,
                "Signal.exe",
                ELECTRON_OUTER_WINDOW_CLASS,
                1_400 * 900,
            ),
            owned(
                0x3002,
                Some(0x3001),
                None,
                6300,
                "Signal.exe",
                ELECTRON_RENDERER_WINDOW_CLASS,
                1_400 * 860,
            ),
        ]
    }

    /// WhatsApp: a WinUI 3 shell whose content is a renderer child inside a
    /// SIBLING `msedgewebview2` process, rooted at the shell's window.
    pub(crate) fn whatsapp_graph() -> Vec<Uia2OwnedWindow> {
        vec![
            owned(
                0x4001,
                None,
                None,
                7400,
                "WhatsApp.exe",
                WHATSAPP_OUTER_WINDOW_CLASS,
                1_300 * 850,
            ),
            owned(
                0x4002,
                Some(0x4001),
                None,
                7400,
                "WhatsApp.exe",
                "Microsoft.UI.Content.DesktopChildSiteBridge",
                1_300 * 840,
            ),
            owned(
                0x5001,
                Some(0x4002),
                Some(0x4001),
                7500,
                "msedgewebview2.exe",
                ELECTRON_OUTER_WINDOW_CLASS,
                1_300 * 830,
            ),
            owned(
                0x5002,
                Some(0x5001),
                Some(0x4001),
                7500,
                "msedgewebview2.exe",
                ELECTRON_RENDERER_WINDOW_CLASS,
                1_300 * 820,
            ),
        ]
    }

    fn discord_plan() -> Uia2WindowPlan {
        Uia2WindowPlan::chromium_outer_msaa_root("Discord", "Discord", 10, 90_000, CALL_TIMEOUT_MS)
    }

    #[test]
    fn producer_binds_the_measured_window_on_all_four_providers() {
        let cases: [(Uia2WindowPlan, Vec<Uia2OwnedWindow>, isize, usize); 4] = [
            (discord_plan(), discord_graph(), 0x1001, 696),
            (
                Uia2WindowPlan::direct_outer_window(
                    "Telegram",
                    "Telegram",
                    TELEGRAM_OUTER_WINDOW_CLASS,
                    CALL_TIMEOUT_MS,
                ),
                telegram_graph(),
                0x2001,
                743,
            ),
            (
                Uia2WindowPlan::chromium_renderer_child("Signal", "Signal", WAIT_MS, CALL_TIMEOUT_MS),
                signal_graph(),
                0x3002,
                49,
            ),
            (
                Uia2WindowPlan::sibling_chromium_renderer(
                    "WhatsApp",
                    "WhatsApp",
                    WHATSAPP_OUTER_WINDOW_CLASS,
                    WEBVIEW2_PROCESS_NAME,
                    WAIT_MS,
                    CALL_TIMEOUT_MS,
                ),
                whatsapp_graph(),
                0x5002,
                11,
            ),
        ];

        for (plan, graph, expected_bound, elements) in cases {
            let mut host = RecordedHost::new(graph, elements);
            if plan.wake_policy == Uia2WakePolicy::WmGetObjectChromium {
                host = host.chromium(2);
            }
            let acquired = acquire_uia2_window(plan, &host)
                .unwrap_or_else(|error| panic!("{} must acquire: {error:?}", plan.provider_name));
            assert_eq!(
                acquired.window.bound_hwnd, expected_bound,
                "{} bound the wrong window",
                plan.provider_name
            );
            assert_eq!(acquired.elements, elements);
            assert_eq!(
                acquired.woke,
                plan.wake_policy == Uia2WakePolicy::WmGetObjectChromium,
                "{} woke against its plan",
                plan.provider_name
            );
            assert!(
                !host.deadlines.borrow().is_empty()
                    && host
                        .deadlines
                        .borrow()
                        .iter()
                        .all(|deadline| *deadline == plan.call_timeout_ms),
                "{}: every cross-process call must carry the plan's deadline, saw {:?}",
                plan.provider_name,
                host.deadlines.borrow()
            );
        }
    }

    #[test]
    fn telegram_never_waits_and_whatsapp_never_binds_the_shell() {
        let plan = Uia2WindowPlan::direct_outer_window(
            "Telegram",
            "Telegram",
            TELEGRAM_OUTER_WINDOW_CLASS,
            CALL_TIMEOUT_MS,
        );
        let host = RecordedHost::new(telegram_graph(), 743);
        let acquired = acquire_uia2_window(plan, &host).expect("Telegram acquires");
        assert_eq!(acquired.settled_ms, 0);
        assert!(!acquired.woke);
        assert!(host.settles.borrow().is_empty(), "Qt needs no settle at all");

        let plan = Uia2WindowPlan::sibling_chromium_renderer(
            "WhatsApp",
            "WhatsApp",
            WHATSAPP_OUTER_WINDOW_CLASS,
            WEBVIEW2_PROCESS_NAME,
            WAIT_MS,
            CALL_TIMEOUT_MS,
        );
        let host = RecordedHost::new(whatsapp_graph(), 11).chromium(1);
        let acquired = acquire_uia2_window(plan, &host).expect("WhatsApp acquires");
        assert_eq!(acquired.window.app_outer_hwnd, 0x4001);
        assert_ne!(
            acquired.window.bound_hwnd, 0x4001,
            "binding the WinUI shell is the dead end that would have written WhatsApp off"
        );
        assert_eq!(acquired.window.bound_process_id, 7500);
    }

    #[test]
    fn the_editables_door_issues_one_call_under_the_plans_own_budget() {
        let plan = Uia2WindowPlan::direct_outer_window(
            "Telegram",
            "Telegram",
            TELEGRAM_OUTER_WINDOW_CLASS,
            CALL_TIMEOUT_MS,
        );
        let host =
            RecordedHost::new(telegram_graph(), 743).with_editables(vec![composer("Message")]);
        let acquired = acquire_uia2_window(plan, &host).expect("Telegram acquires");
        host.deadlines.borrow_mut().clear();

        let editables =
            acquire_uia2_editables(&host, acquired).expect("the editable scan must answer");
        assert_eq!(editables.len(), 1);
        assert_eq!(
            *host.deadlines.borrow(),
            vec![CALL_TIMEOUT_MS],
            "the door must issue exactly one cross-process call, under the deadline the \
             acquisition derived from the plan -- not a wider one it chose for itself"
        );
    }

    #[test]
    fn the_editables_door_abandons_a_provider_that_stops_answering() {
        // Mutant [1] in shape: an unbounded editable scan is the one call that
        // could freeze OSL against another application's UI thread. Starve it
        // and the budget it spent has to be the plan's, to the millisecond.
        let plan = Uia2WindowPlan::direct_outer_window(
            "Telegram",
            "Telegram",
            TELEGRAM_OUTER_WINDOW_CLASS,
            CALL_TIMEOUT_MS,
        );
        let host =
            RecordedHost::new(telegram_graph(), 743).with_editables(vec![composer("Message")]);
        let acquired = acquire_uia2_window(plan, &host).expect("Telegram acquires");

        let mut starved =
            RecordedHost::new(telegram_graph(), 743).with_editables(vec![composer("Message")]);
        starved.never_answers = true;
        assert_eq!(
            acquire_uia2_editables(&starved, acquired),
            Err(Uia2CallTimeout {
                timeout_ms: CALL_TIMEOUT_MS
            }),
            "a scan that never answers must cost the plan's budget and no more"
        );
    }

    /// D-155 mutant [2]. The whole timeout guarantee rests on `Uia2Deadline`
    /// being unconstructable outside this module, and **nothing else in the
    /// tree fails if that privacy is relaxed**: Rust has no runtime handle on
    /// visibility, so a consumer that gained the ability to invent its own
    /// budget would simply compile, silently. Measured, not assumed -- making
    /// both the field and `from_plan` public left all 321 tests across the
    /// `native_a11y`, `native_telegram_adapter` and `native_discord_adapter`
    /// filters green.
    ///
    /// So it is pinned at the only place it is observable: the source.
    #[test]
    fn the_deadline_token_has_no_public_constructor() {
        // Production code only. This module's own test source quotes the
        // declaration it is checking, so scanning the whole file would make
        // every assertion below satisfy itself.
        let production = include_str!("native_a11y.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default();
        assert!(
            production.contains("pub fn place_uia2_carrier"),
            "the scanned region lost its production code, so the scan is vacuous"
        );
        assert!(
            !production.contains("fn the_deadline_token_has_no_public_constructor"),
            "the test half leaked into the scan, so the scan can satisfy itself"
        );

        // The scanner must be able to fire, or this guard is decoration.
        assert!(hands_back_a_deadline(
            "    pub fn new(millis: u64) -> Self {"
        ));
        assert!(hands_back_a_deadline(
            "    pub const fn of(ms: u64) -> Uia2Deadline {"
        ));
        assert!(!hands_back_a_deadline("    pub fn millis(self) -> u64 {"));
        assert!(!hands_back_a_deadline(
            "    fn from_plan(plan: Uia2WindowPlan) -> Self {"
        ));

        assert!(
            production.contains("pub struct Uia2Deadline(u64);"),
            "the deadline's field must stay private -- a `pub` field IS a public \
             constructor, and the token would stop meaning anything"
        );

        let inherent = production
            .split_once("\nimpl Uia2Deadline {\n")
            .expect("the deadline's inherent impl must exist for this scan to mean anything")
            .1
            .split_once("\n}\n")
            .expect("the deadline's inherent impl must be terminated")
            .0;
        assert!(
            inherent.contains("fn from_plan"),
            "the scanned impl lost its body, so the scan is vacuous"
        );
        for line in inherent.lines() {
            assert!(
                !hands_back_a_deadline(line),
                "a public associated function handing back a deadline is a public \
                 constructor by another name, which is the one thing this token exists \
                 to prevent: {line:?}"
            );
        }

        // A trait impl is as public as its trait, so `From<u64> for Uia2Deadline`
        // would be the same hole through a different door.
        assert!(
            !production.contains("for Uia2Deadline {"),
            "no trait impl may build a deadline out of a caller's own number"
        );
    }

    /// True for a source line that publicly hands back a [`Uia2Deadline`].
    fn hands_back_a_deadline(line: &str) -> bool {
        let trimmed = line.trim();
        (trimmed.starts_with("pub fn ") || trimmed.starts_with("pub const fn "))
            && (trimmed.contains("-> Self") || trimmed.contains("-> Uia2Deadline"))
    }

    #[test]
    fn a_chromium_provider_that_is_never_woken_never_populates() {
        // Mutant [3] in shape: the wake is what makes the tree exist. Skipping
        // it must not be survivable by waiting longer.
        let plan = Uia2WindowPlan::chromium_renderer_no_wake_mutant(
            "Signal",
            "Signal",
            WAIT_MS,
            CALL_TIMEOUT_MS,
        );
        let host = RecordedHost::new(signal_graph(), 49).chromium(0);

        assert_eq!(
            acquire_uia2_window(plan, &host),
            Err(Uia2AcquireError::TreeNeverPopulated {
                seen: 1,
                needed: ELECTRON_UIA2_POPULATED_MIN_ELEMENTS,
            })
        );
        assert!(
            !host.settles.borrow().is_empty(),
            "the poll must actually have waited before giving up"
        );
    }

    #[test]
    fn a_provider_that_stops_answering_times_out_rather_than_hanging() {
        let plan = discord_plan();
        let mut host = RecordedHost::new(discord_graph(), 696);
        host.never_answers = true;

        assert_eq!(
            acquire_uia2_window(plan, &host),
            Err(Uia2AcquireError::CallTimedOut(Uia2CallTimeout {
                timeout_ms: CALL_TIMEOUT_MS,
            }))
        );
    }

    #[test]
    fn a_refused_wake_is_not_a_timeout() {
        let plan = discord_plan();
        let mut host = RecordedHost::new(discord_graph(), 696).chromium(0);
        host.wake_answers = false;

        assert_eq!(
            acquire_uia2_window(plan, &host),
            Err(Uia2AcquireError::WakeRefused)
        );
    }

    #[test]
    fn the_settle_plan_spends_its_budget_and_no_more() {
        assert_eq!(uia2_settle_plan(0), Vec::<u64>::new());
        assert_eq!(uia2_settle_plan(100), vec![100]);
        assert_eq!(uia2_settle_plan(1_000), vec![150, 300, 500, 50]);
        let plan = uia2_settle_plan(90_000);
        assert_eq!(plan.iter().sum::<u64>(), 90_000);
        assert!(
            plan.len() < 20,
            "a 90 s budget must not be spent 150 ms at a time: {plan:?}"
        );
    }

    #[test]
    fn a_search_box_is_not_a_composer() {
        let elements = vec![composer("Search messages"), composer("Filter")];
        assert_eq!(
            resolve_uia2_composer(MATCHER, &elements),
            Err(Uia2ComposerError::NoComposerName)
        );

        let elements = vec![composer("Search messages"), composer("Write a message...")];
        assert_eq!(
            resolve_uia2_composer(MATCHER, &elements)
                .expect("Telegram's composer resolves")
                .name,
            "Write a message..."
        );
    }

    #[test]
    fn a_composer_that_cannot_be_written_is_not_a_composer() {
        let mut read_only = composer("Write a message...");
        read_only.read_only = true;
        assert_eq!(
            resolve_uia2_composer(MATCHER, &[read_only]),
            Err(Uia2ComposerError::NoWritableEditable)
        );

        let mut unfocusable = composer("Write a message...");
        unfocusable.keyboard_focusable = false;
        assert_eq!(
            resolve_uia2_composer(MATCHER, &[unfocusable]),
            Err(Uia2ComposerError::NoWritableEditable)
        );

        assert_eq!(
            resolve_uia2_composer(MATCHER, &[]),
            Err(Uia2ComposerError::NoEditable)
        );
    }

    #[test]
    fn two_composers_are_refused_rather_than_guessed_at() {
        let elements = vec![composer("Message @liam"), composer("Message #general")];
        assert_eq!(
            resolve_uia2_composer(MATCHER, &elements),
            Err(Uia2ComposerError::Ambiguous(2))
        );
    }

    fn telegram_session() -> (RecordedHost, Uia2Acquired, Uia2Editable) {
        let plan = Uia2WindowPlan::direct_outer_window(
            "Telegram",
            "Telegram",
            TELEGRAM_OUTER_WINDOW_CLASS,
            CALL_TIMEOUT_MS,
        );
        let element = composer("Write a message...");
        let host = RecordedHost::new(telegram_graph(), 743).with_editables(vec![element.clone()]);
        let acquired = acquire_uia2_window(plan, &host).expect("Telegram acquires");
        (host, acquired, element)
    }

    #[test]
    fn placement_reads_back_with_contains_because_a_live_ui_augments_its_own_fields() {
        let (mut host, acquired, element) = telegram_session();
        host.readback_suffix = " and a suggestion the app appended";

        let receipt = place_uia2_carrier(&host, acquired, &element, "alpha-7731-osl", false)
            .expect("the carrier is placed");

        assert!(receipt.placed);
        assert!(receipt.readback_holds_carrier);
        assert!(!receipt.submit_shaped_observed);
        assert_eq!(host.set_values.borrow().as_slice(), ["alpha-7731-osl"]);

        clear_uia2_composer(&host, acquired, &element).expect("the composer clears");
        assert_eq!(
            host.set_values.borrow().as_slice(),
            ["alpha-7731-osl", ""],
            "a probe must always clear after itself"
        );
    }

    #[test]
    fn a_carrier_that_carries_a_line_break_never_reaches_the_composer() {
        let (host, acquired, element) = telegram_session();

        assert_eq!(
            place_uia2_carrier(&host, acquired, &element, "alpha-7731-osl\nsend", false),
            Err(Uia2PlacementRefusal::CarrierCarriesSubmit)
        );
        assert_eq!(
            place_uia2_carrier(&host, acquired, &element, "alpha\u{2028}osl", false),
            Err(Uia2PlacementRefusal::CarrierCarriesSubmit)
        );
        assert!(
            host.set_values.borrow().is_empty(),
            "nothing may be written before the carrier is judged"
        );
    }

    #[test]
    fn the_submit_shaped_verdict_can_be_true_so_the_guard_can_bite() {
        // A receipt field that cannot be false is decoration -- D-139 finding 2.
        // This proves the backend's counter is what decides it.
        let (mut host, acquired, element) = telegram_session();
        host.submit_shaped_on_set = true;

        assert_eq!(
            place_uia2_carrier(&host, acquired, &element, "alpha-7731-osl", false),
            Err(Uia2PlacementRefusal::SubmitShaped)
        );
        assert_eq!(host.submit_shaped_calls(), 1);

        let (host, acquired, element) = telegram_session();
        let receipt = place_uia2_carrier(&host, acquired, &element, "alpha-7731-osl", false)
            .expect("an unremarkable placement still succeeds");
        assert!(!receipt.submit_shaped_observed);
    }

    #[test]
    fn placement_refuses_to_overwrite_a_draft_the_operator_wrote() {
        let (host, acquired, element) = telegram_session();
        *host.value.borrow_mut() = Some("half a sentence the owner typed".to_owned());

        assert_eq!(
            place_uia2_carrier(&host, acquired, &element, "alpha-7731-osl", false),
            Err(Uia2PlacementRefusal::ExistingDraft)
        );
        assert!(host.set_values.borrow().is_empty());

        place_uia2_carrier(&host, acquired, &element, "alpha-7731-osl", true)
            .expect("an explicit replace is allowed");
    }

    #[test]
    fn a_write_that_does_not_read_back_is_refused() {
        // A composer that swallows the write: `SetValue` answers yes and the
        // value never becomes the carrier. Placement must not report success.
        struct Swallowing(RecordedHost);
        impl Uia2Syscalls for Swallowing {
            fn enumerate_windows(
                &self,
                deadline: Uia2Deadline,
            ) -> Result<Vec<Uia2OwnedWindow>, Uia2CallTimeout> {
                self.0.enumerate_windows(deadline)
            }
            fn wake_chromium(
                &self,
                hwnd: isize,
                deadline: Uia2Deadline,
            ) -> Result<bool, Uia2CallTimeout> {
                self.0.wake_chromium(hwnd, deadline)
            }
            fn element_count(
                &self,
                hwnd: isize,
                route: Uia2TreeRoute,
                deadline: Uia2Deadline,
            ) -> Result<usize, Uia2CallTimeout> {
                self.0.element_count(hwnd, route, deadline)
            }
            fn editable_elements(
                &self,
                hwnd: isize,
                route: Uia2TreeRoute,
                deadline: Uia2Deadline,
            ) -> Result<Vec<Uia2Editable>, Uia2CallTimeout> {
                self.0.editable_elements(hwnd, route, deadline)
            }
            fn set_value(
                &self,
                _hwnd: isize,
                _route: Uia2TreeRoute,
                _element: &Uia2Editable,
                _value: &str,
                _deadline: Uia2Deadline,
            ) -> Result<bool, Uia2CallTimeout> {
                Ok(true)
            }
            fn value_of(
                &self,
                _hwnd: isize,
                _route: Uia2TreeRoute,
                _element: &Uia2Editable,
                _deadline: Uia2Deadline,
            ) -> Result<Option<String>, Uia2CallTimeout> {
                Ok(None)
            }
            fn submit_shaped_calls(&self) -> usize {
                0
            }
            fn settle(&self, millis: u64) {
                self.0.settle(millis);
            }
        }

        let (host, acquired, element) = telegram_session();
        let swallowing = Swallowing(host);
        assert_eq!(
            place_uia2_carrier(&swallowing, acquired, &element, "alpha-7731-osl", false),
            Err(Uia2PlacementRefusal::ReadbackMissingCarrier)
        );
    }

    /// Drive a REAL composer through this substrate on a Windows host.
    ///
    /// This is the only thing here that touches a live provider, and it cannot
    /// run in this lane: `win32` is `cfg(target_os = "windows")` and this
    /// machine is Linux. It is `#[ignore]`d so it never runs unattended, and it
    /// reads its provider from the environment so it can be pointed at each of
    /// the four in turn.
    ///
    /// ```text
    /// # from WSL, build the Windows test binary:
    /// flock /tmp/osl-cargo.lock cargo test --manifest-path apps/osl-hub/Cargo.toml \
    ///   --lib --target x86_64-pc-windows-gnu -j 4 --no-run
    /// # then, on the Windows host, with the provider open and signed in:
    /// set OSL_UIA2_PROBE=telegram
    /// osl_hub-<hash>.exe --ignored --test-threads=1 --nocapture drive_a_real_composer
    /// ```
    ///
    /// Placement only. Setting `OSL_UIA2_PROBE_CARRIER` is what opts into a
    /// write; the composer is cleared immediately afterwards, and nothing here
    /// can commit -- there is no verb in `Uia2Syscalls` that could.
    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "drives a live provider on a Windows host; run explicitly"]
    fn drive_a_real_composer_through_the_substrate() {
        let provider = std::env::var("OSL_UIA2_PROBE").unwrap_or_else(|_| "telegram".to_owned());
        const PROBE_CALL_TIMEOUT_MS: u64 = 5_000;
        let plan = match provider.as_str() {
            "discord" => Uia2WindowPlan::chromium_outer_msaa_root(
                "Discord",
                "Discord",
                ELECTRON_UIA2_POPULATED_MIN_ELEMENTS,
                90_000,
                PROBE_CALL_TIMEOUT_MS,
            ),
            "telegram" => Uia2WindowPlan::direct_outer_window(
                "Telegram",
                "Telegram",
                TELEGRAM_OUTER_WINDOW_CLASS,
                PROBE_CALL_TIMEOUT_MS,
            ),
            "signal" => Uia2WindowPlan::chromium_renderer_child(
                "Signal",
                "Signal",
                90_000,
                PROBE_CALL_TIMEOUT_MS,
            ),
            // The running image is `WhatsApp.Root.exe`, not `WhatsApp.exe`, so
            // the literal that used to sit here matched nothing and this probe
            // would have reported `MissingAppOuter` -- "not running" -- on a
            // machine where WhatsApp was on screen. The process name and the
            // window class come from the shipping plan; only the per-call budget
            // is the probe's own, as it is for the other three.
            "whatsapp" => Uia2WindowPlan::sibling_chromium_renderer(
                crate::native_whatsapp_adapter::WHATSAPP_UIA2_WINDOW_PLAN.provider_name,
                crate::native_whatsapp_adapter::WHATSAPP_UIA2_WINDOW_PLAN.app_process_name,
                crate::native_whatsapp_adapter::WHATSAPP_UIA2_WINDOW_PLAN.app_outer_class,
                WEBVIEW2_PROCESS_NAME,
                90_000,
                PROBE_CALL_TIMEOUT_MS,
            ),
            other => panic!("OSL_UIA2_PROBE={other} is not one of discord|telegram|signal|whatsapp"),
        };

        let host = win32::Uia2Win32Host::desktop();
        let acquired = acquire_uia2_window(plan, &host)
            .unwrap_or_else(|error| panic!("{provider}: acquire failed: {error:?}"));
        eprintln!(
            "{provider}: bound_hwnd=<redacted> pid={} elements={} woke={} settled_ms={}",
            acquired.window.bound_process_id, acquired.elements, acquired.woke, acquired.settled_ms
        );

        let editables = host
            .editable_elements(
                acquired.window.bound_hwnd,
                acquired.window.tree_route,
                Uia2Deadline(acquired.window.call_timeout_ms),
            )
            .expect("the editable scan must answer inside its deadline");
        eprintln!(
            "{provider}: editable={} writable={}",
            editables.len(),
            editables.iter().filter(|element| element.writable()).count()
        );
        for element in &editables {
            eprintln!(
                "  edit name={:?} value_pattern={} enabled={} kbd={} read_only={}",
                element.name,
                element.value_pattern,
                element.enabled,
                element.keyboard_focusable,
                element.read_only
            );
        }

        let composer = resolve_uia2_composer(MATCHER, &editables)
            .unwrap_or_else(|error| panic!("{provider}: no composer resolved: {error:?}"));
        eprintln!("{provider}: composer name={:?}", composer.name);

        let Ok(carrier) = std::env::var("OSL_UIA2_PROBE_CARRIER") else {
            eprintln!("{provider}: read-only probe, nothing written");
            return;
        };
        let receipt = place_uia2_carrier(&host, acquired, &composer, &carrier, false)
            .unwrap_or_else(|error| panic!("{provider}: placement refused: {error:?}"));
        eprintln!(
            "{provider}: placed={} readback_holds_carrier={} submit_shaped={}",
            receipt.placed, receipt.readback_holds_carrier, receipt.submit_shaped_observed
        );
        clear_uia2_composer(&host, acquired, &composer)
            .unwrap_or_else(|error| panic!("{provider}: the composer did not clear: {error:?}"));
        eprintln!("{provider}: composer cleared -- nothing was sent");
    }

    #[test]
    fn discords_plan_is_not_the_outer_window_mutant() {
        let discord = discord_plan();
        let mutant =
            Uia2WindowPlan::chromium_outer_mutant("Discord", "Discord", 90_000, CALL_TIMEOUT_MS);

        assert_eq!(discord.shape, mutant.shape);
        assert_eq!(discord.wake_policy, mutant.wake_policy);
        assert_eq!(
            discord.tree_route,
            Uia2TreeRoute::MsaaBridge,
            "Discord's proven route is Chromium's MSAA client object, not a UIA tree"
        );
        assert_eq!(
            mutant.tree_route,
            Uia2TreeRoute::UiaNative,
            "the mutant is only a mutant because it asks UI Automation for a tree \
             at a window A-00 measured as blind"
        );
        assert_ne!(discord, mutant);
    }

    // -----------------------------------------------------------------
    // D-156: associating a sibling WebView2 by PROCESS parentage.
    //
    // Every number below was measured on the owner's Windows host, not
    // invented. The window graph:
    //
    //   198342 pid=23884 WhatsApp.Root   WinUIDesktopWin32WindowClass  parent=0
    //   197328 pid=23884 WhatsApp.Root   ...DesktopChildSiteBridge     parent=198342
    //    67446 pid=24196 msedgewebview2  Chrome_WidgetWin_1            parent=0
    //   132018 pid=24196 msedgewebview2  Chrome_RenderWidgetHostHWND   parent=67446
    //
    // and the process table, which is where the only real link lives:
    //
    //   ProcessId  ParentProcessId  parentName      --webview-exe-name
    //       24196            23884  WhatsApp.Root   WhatsApp.Root.exe
    //       22824            21944  SearchApp       SearchApp.exe
    //       11744            22824  msedgewebview2  SearchApp.exe
    //
    // The decoy is not a hypothetical. Windows Search hosts its own WebView2 on
    // this machine, at full screen, and it is the LARGER window of the two.
    // -----------------------------------------------------------------

    const MEASURED_SHELL_HWND: isize = 198_342;
    const MEASURED_WEBVIEW_OUTER_HWND: isize = 67_446;
    const MEASURED_RENDERER_HWND: isize = 132_018;
    const DECOY_OUTER_HWND: isize = 900_001;
    const DECOY_RENDERER_HWND: isize = 900_002;
    const MEASURED_SHELL_PID: u32 = 23_884;
    const MEASURED_WEBVIEW_PID: u32 = 24_196;
    const DECOY_SEARCHAPP_PID: u32 = 22_824;
    const DECOY_WEBVIEW_PID: u32 = 11_744;
    const MEASURED_SHELL_IMAGE: &str = "WhatsApp.Root.exe";
    const MEASURED_SHELL_PROCESS: &str = "WhatsApp.Root";
    const MEASURED_SHELL_AREA: u32 = 1_092_960;
    const MEASURED_WEBVIEW_AREA: u32 = 1_068_000;
    /// Full screen. Deliberately bigger than WhatsApp's.
    const DECOY_AREA: u32 = 1_920 * 1_080;

    fn measured_whatsapp_plan() -> Uia2WindowPlan {
        Uia2WindowPlan::sibling_chromium_renderer(
            "WhatsApp",
            MEASURED_SHELL_PROCESS,
            WHATSAPP_OUTER_WINDOW_CLASS,
            WEBVIEW2_PROCESS_NAME,
            WAIT_MS,
            CALL_TIMEOUT_MS,
        )
    }

    /// The owner's host as measured: WhatsApp's shell and its WebView2, plus
    /// Windows Search's WebView2 with the larger window. No window in this graph
    /// carries `associated_app_hwnd` -- that is the finding.
    fn two_webview2_hosts_decoy_larger() -> Vec<Uia2WindowCandidate<'static>> {
        vec![
            hosted_window(
                MEASURED_SHELL_HWND,
                None,
                MEASURED_SHELL_PID,
                9_644,
                None,
                MEASURED_SHELL_IMAGE,
                WHATSAPP_OUTER_WINDOW_CLASS,
                MEASURED_SHELL_AREA,
            ),
            hosted_window(
                197_328,
                Some(MEASURED_SHELL_HWND),
                MEASURED_SHELL_PID,
                9_644,
                None,
                MEASURED_SHELL_IMAGE,
                "Microsoft.UI.Content.DesktopChildSiteBridge",
                MEASURED_WEBVIEW_AREA,
            ),
            hosted_window(
                MEASURED_WEBVIEW_OUTER_HWND,
                None,
                MEASURED_WEBVIEW_PID,
                MEASURED_SHELL_PID,
                Some("WhatsApp.Root.exe"),
                "msedgewebview2.exe",
                ELECTRON_OUTER_WINDOW_CLASS,
                MEASURED_WEBVIEW_AREA,
            ),
            hosted_window(
                MEASURED_RENDERER_HWND,
                Some(MEASURED_WEBVIEW_OUTER_HWND),
                MEASURED_WEBVIEW_PID,
                MEASURED_SHELL_PID,
                Some("WhatsApp.Root.exe"),
                "msedgewebview2.exe",
                ELECTRON_RENDERER_WINDOW_CLASS,
                MEASURED_WEBVIEW_AREA,
            ),
            hosted_window(
                DECOY_OUTER_HWND,
                None,
                DECOY_WEBVIEW_PID,
                DECOY_SEARCHAPP_PID,
                Some("SearchApp.exe"),
                "msedgewebview2.exe",
                ELECTRON_OUTER_WINDOW_CLASS,
                DECOY_AREA,
            ),
            hosted_window(
                DECOY_RENDERER_HWND,
                Some(DECOY_OUTER_HWND),
                DECOY_WEBVIEW_PID,
                DECOY_SEARCHAPP_PID,
                Some("SearchApp.exe"),
                "msedgewebview2.exe",
                ELECTRON_RENDERER_WINDOW_CLASS,
                DECOY_AREA,
            ),
        ]
    }

    #[test]
    fn the_measured_whatsapp_graph_resolves_by_process_parentage() {
        let windows = two_webview2_hosts_decoy_larger();

        // The premise first, or this test proves nothing: on the real machine
        // there is NO window-tree link at all. If a future fixture quietly
        // reintroduces one, the resolution below stops being evidence for
        // parentage.
        assert!(
            windows
                .iter()
                .all(|window| window.associated_app_hwnd.is_none()),
            "the measured graph has no window-tree association; a fixture that \
             adds one is testing the old contract, not the machine"
        );

        let resolved = resolve_uia2_window(measured_whatsapp_plan(), &windows)
            .expect("WhatsApp's WebView2 must resolve through process parentage");

        assert_eq!(resolved.app_outer_hwnd, MEASURED_SHELL_HWND);
        assert_eq!(resolved.bound_hwnd, MEASURED_RENDERER_HWND);
        assert_eq!(resolved.bound_process_id, MEASURED_WEBVIEW_PID);
        assert_ne!(
            resolved.bound_process_id, DECOY_WEBVIEW_PID,
            "binding Windows Search's WebView2 would place an OSL carrier into \
             the Search box"
        );
    }

    /// Mutant [1]: select by largest visible window. The decoy is chosen.
    ///
    /// The mutant is computed here rather than left in production, so the
    /// artifact is a number this test can print: the "obvious" rule picks
    /// Windows Search, at a larger area, in a different process.
    #[test]
    fn largest_visible_webview2_is_the_decoy_not_whatsapp() {
        let windows = two_webview2_hosts_decoy_larger();
        let plan = measured_whatsapp_plan();

        let largest_by_area = windows
            .iter()
            .copied()
            .filter(|window| {
                window.process_name.starts_with(WEBVIEW2_PROCESS_NAME)
                    && window.class_name == ELECTRON_OUTER_WINDOW_CLASS
                    && window.visible
            })
            .max_by_key(|window| window.area)
            .expect("both WebView2 hosts are in the graph");

        assert_eq!(
            largest_by_area.hwnd, DECOY_OUTER_HWND,
            "mutant [1] must actually be wrong here, or the fixture is not the \
             danger it claims to be"
        );
        assert_eq!(largest_by_area.process_id, DECOY_WEBVIEW_PID);
        assert!(largest_by_area.area > MEASURED_WEBVIEW_AREA);

        let resolved =
            resolve_uia2_window(plan, &windows).expect("the resolver must still find WhatsApp's");
        assert_ne!(resolved.bound_process_id, largest_by_area.process_id);
        assert_eq!(resolved.bound_process_id, MEASURED_WEBVIEW_PID);
    }

    /// Mutant [2]: match on process NAME only, ignoring parentage.
    ///
    /// Both hosts are `msedgewebview2.exe`. The name cannot separate them, and a
    /// resolver that tried would be choosing between two equally valid answers.
    #[test]
    fn the_process_name_alone_cannot_tell_the_two_webview2_hosts_apart() {
        let windows = two_webview2_hosts_decoy_larger();

        let by_name_only = windows
            .iter()
            .filter(|window| {
                window.class_name == ELECTRON_OUTER_WINDOW_CLASS
                    && window.process_name.starts_with(WEBVIEW2_PROCESS_NAME)
            })
            .count();
        assert_eq!(
            by_name_only, 2,
            "the image name matches both hosts, which is why it is not the link"
        );

        let shell = windows
            .iter()
            .copied()
            .find(|window| window.hwnd == MEASURED_SHELL_HWND)
            .expect("the shell is in the graph");
        let ours = windows
            .iter()
            .copied()
            .find(|window| window.hwnd == MEASURED_WEBVIEW_OUTER_HWND)
            .expect("WhatsApp's host is in the graph");
        let theirs = windows
            .iter()
            .copied()
            .find(|window| window.hwnd == DECOY_OUTER_HWND)
            .expect("the decoy is in the graph");

        assert_eq!(ours.process_name, theirs.process_name);
        assert_eq!(
            classify_sibling_host(shell, ours),
            SiblingHostAssociation::OwnedByApp
        );
        assert_eq!(
            classify_sibling_host(shell, theirs),
            SiblingHostAssociation::Foreign,
            "the decoy is another application's WebView2, not an error state"
        );
    }

    /// The measured graph plus one more visible `Chrome_WidgetWin_1` in a third
    /// process, described by the caller.
    ///
    /// Every refusal below needs this shape rather than a mutated WhatsApp host.
    /// D-181: mutating the only parented host empties `owned_hosts` either way,
    /// so the resolver reaches the same refusal whether or not its
    /// `Contradicted | Uncorroborated` branch exists at all -- the test pins the
    /// classifier's enum and nothing about what the resolver does with it.
    /// Keeping WhatsApp's real host intact is what makes the branch
    /// consequential: with it, `Err`; without it, a bind.
    fn measured_graph_plus_extra_host(
        outer_hwnd: isize,
        renderer_hwnd: isize,
        process_id: u32,
        parent_process_id: u32,
        host_exe_name: Option<&'static str>,
        visible: bool,
        area: u32,
    ) -> Vec<Uia2WindowCandidate<'static>> {
        let mut windows = two_webview2_hosts_decoy_larger();
        let mut outer = hosted_window(
            outer_hwnd,
            None,
            process_id,
            parent_process_id,
            host_exe_name,
            "msedgewebview2.exe",
            ELECTRON_OUTER_WINDOW_CLASS,
            area,
        );
        let mut renderer = hosted_window(
            renderer_hwnd,
            Some(outer_hwnd),
            process_id,
            parent_process_id,
            host_exe_name,
            "msedgewebview2.exe",
            ELECTRON_RENDERER_WINDOW_CLASS,
            area,
        );
        outer.visible = visible;
        renderer.visible = visible;
        windows.push(outer);
        windows.push(renderer);
        windows
    }

    /// The graph still contains WhatsApp's own host, so the resolver has
    /// something legitimate it COULD bind. Anything else and the refusal is
    /// indistinguishable from "there was nothing to bind".
    fn the_legitimate_host_is_still_present(windows: &[Uia2WindowCandidate<'_>]) {
        let shell = windows
            .iter()
            .copied()
            .find(|window| window.hwnd == MEASURED_SHELL_HWND)
            .expect("the shell is in the graph");
        let ours = windows
            .iter()
            .copied()
            .find(|window| window.hwnd == MEASURED_WEBVIEW_OUTER_HWND)
            .expect("WhatsApp's own host is in the graph");
        assert_eq!(
            classify_sibling_host(shell, ours),
            SiblingHostAssociation::OwnedByApp,
            "the refusal under test must be a refusal to choose, not a refusal \
             for want of anything to choose"
        );
    }

    /// Mutant [3]: drop the `--webview-exe-name` corroboration.
    ///
    /// A process parented to WhatsApp that declares it is hosting Windows Search
    /// is a state nobody has observed. WhatsApp's real host is left in the graph
    /// and resolvable, so the resolver has a legitimate answer available and
    /// declines to give it: the contradiction poisons the whole graph rather
    /// than being skipped over.
    #[test]
    fn a_contradicted_host_refuses_even_though_a_legitimate_one_is_present() {
        const IMPOSTOR_OUTER: isize = 700_001;
        const IMPOSTOR_RENDERER: isize = 700_002;
        const IMPOSTOR_PID: u32 = 77_777;

        let windows = measured_graph_plus_extra_host(
            IMPOSTOR_OUTER,
            IMPOSTOR_RENDERER,
            IMPOSTOR_PID,
            MEASURED_SHELL_PID,
            Some("SearchApp.exe"),
            true,
            DECOY_AREA,
        );
        the_legitimate_host_is_still_present(&windows);

        let shell = windows
            .iter()
            .copied()
            .find(|window| window.hwnd == MEASURED_SHELL_HWND)
            .expect("the shell is in the graph");
        let contradicted = windows
            .iter()
            .copied()
            .find(|window| window.hwnd == IMPOSTOR_OUTER)
            .expect("the impostor is in the graph");
        assert_eq!(contradicted.parent_process_id, shell.process_id);
        assert_eq!(
            classify_sibling_host(shell, contradicted),
            SiblingHostAssociation::Contradicted
        );

        assert_eq!(
            resolve_uia2_window(measured_whatsapp_plan(), &windows),
            Err(Uia2WindowResolveError::MissingSiblingContentOuter),
            "a host parented here while naming another application must refuse \
             the whole resolution, not be quietly skipped in favour of the one \
             that agrees"
        );
    }

    /// Corroboration is required, not merely checked when convenient. Same
    /// shape: WhatsApp's own host stays resolvable, and the uncorroborated
    /// sibling still refuses the resolution rather than being stepped over.
    #[test]
    fn an_uncorroborated_host_refuses_even_though_a_legitimate_one_is_present() {
        const UNREADABLE_OUTER: isize = 710_001;
        const UNREADABLE_RENDERER: isize = 710_002;
        const UNREADABLE_PID: u32 = 78_888;

        let windows = measured_graph_plus_extra_host(
            UNREADABLE_OUTER,
            UNREADABLE_RENDERER,
            UNREADABLE_PID,
            MEASURED_SHELL_PID,
            None,
            true,
            MEASURED_WEBVIEW_AREA / 2,
        );
        the_legitimate_host_is_still_present(&windows);

        let shell = windows
            .iter()
            .copied()
            .find(|window| window.hwnd == MEASURED_SHELL_HWND)
            .expect("the shell is in the graph");
        let uncorroborated = windows
            .iter()
            .copied()
            .find(|window| window.hwnd == UNREADABLE_OUTER)
            .expect("the unreadable host is in the graph");
        assert_eq!(
            classify_sibling_host(shell, uncorroborated),
            SiblingHostAssociation::Uncorroborated
        );

        assert_eq!(
            resolve_uia2_window(measured_whatsapp_plan(), &windows),
            Err(Uia2WindowResolveError::MissingSiblingContentOuter),
            "an unreadable command line on a parented sibling must refuse, not \
             fall through to the host that happens to be readable"
        );
    }

    /// D-180: the anchor is correct but it is not a UNIQUE key. Two hosts can
    /// satisfy both signals at once, and this is what happens then.
    ///
    /// The second host is full-screen, so a size tie-break binds it: against
    /// unmutated production code the adversary measured `bound_hwnd=700002
    /// bound_pid=77777` where the real WhatsApp is `132018 / 24196`. There is no
    /// second line of defence downstream -- the placement path checks only that
    /// the bound window is not the app shell -- so a wrong bind here puts the
    /// payload's carrier in a window the user never chose.
    #[test]
    fn two_qualifying_hosts_refuse_rather_than_picking_the_larger() {
        const SECOND_OUTER: isize = 700_001;
        const SECOND_RENDERER: isize = 700_002;
        const SECOND_PID: u32 = 77_777;

        // Both signals satisfied, exactly like WhatsApp's own host.
        let windows = measured_graph_plus_extra_host(
            SECOND_OUTER,
            SECOND_RENDERER,
            SECOND_PID,
            MEASURED_SHELL_PID,
            Some("WhatsApp.Root.exe"),
            true,
            DECOY_AREA,
        );
        the_legitimate_host_is_still_present(&windows);

        let shell = windows
            .iter()
            .copied()
            .find(|window| window.hwnd == MEASURED_SHELL_HWND)
            .expect("the shell is in the graph");
        let owned: Vec<_> = windows
            .iter()
            .copied()
            .filter(|window| {
                window.class_name == ELECTRON_OUTER_WINDOW_CLASS
                    && window.visible
                    && classify_sibling_host(shell, *window) == SiblingHostAssociation::OwnedByApp
            })
            .collect();
        assert_eq!(
            owned.len(),
            2,
            "the fixture must actually be ambiguous, or it is not this case"
        );

        // What a size tie-break would have done, stated as a number so the
        // danger is not left implicit.
        let by_size = owned
            .iter()
            .copied()
            .max_by_key(|window| window.area)
            .expect("two owned hosts");
        assert_eq!(
            by_size.hwnd, SECOND_OUTER,
            "the larger of the two owned hosts is NOT WhatsApp's, which is why \
             size cannot be the tie-break"
        );

        assert_eq!(
            resolve_uia2_window(measured_whatsapp_plan(), &windows),
            Err(Uia2WindowResolveError::MissingSiblingContentOuter),
            "ambiguity must fail closed: with two hosts satisfying both signals \
             the resolver has no evidence which holds the conversation, and \
             `largest_visible` was already rejected as mutant [1]"
        );

        // And it is ambiguity that refuses, not the mere presence of a second
        // window: drop the impostor and the same plan binds WhatsApp's host.
        assert_eq!(
            resolve_uia2_window(
                measured_whatsapp_plan(),
                &two_webview2_hosts_decoy_larger()
            )
            .expect("one owned host resolves")
            .bound_hwnd,
            MEASURED_RENDERER_HWND
        );
    }

    /// D-181, the denial-of-function half: a window OSL could never bind must
    /// not be able to veto one it can.
    ///
    /// The live enumeration returns several invisible zero-area `msedgewebview2`
    /// windows in WhatsApp's own process tree -- `crashpad_SessionEndWatcher`,
    /// `IME`, `Chrome_WidgetWin_0` -- and a `Chrome_WidgetWin_1` can be among
    /// them while the app is closed to the tray. Without a visibility filter one
    /// of those, with an unreadable command line, refuses a healthy WhatsApp.
    #[test]
    fn an_invisible_sibling_cannot_veto_a_healthy_whatsapp() {
        let windows = measured_graph_plus_extra_host(
            720_001,
            720_002,
            79_999,
            MEASURED_SHELL_PID,
            None,
            false,
            0,
        );
        the_legitimate_host_is_still_present(&windows);

        let shell = windows
            .iter()
            .copied()
            .find(|window| window.hwnd == MEASURED_SHELL_HWND)
            .expect("the shell is in the graph");
        let invisible = windows
            .iter()
            .copied()
            .find(|window| window.hwnd == 720_001)
            .expect("the invisible sibling is in the graph");
        assert!(!invisible.visible);
        assert_eq!(
            classify_sibling_host(shell, invisible),
            SiblingHostAssociation::Uncorroborated,
            "the classifier still says what it sees; it is the resolver that \
             must never have asked about a window it cannot bind"
        );

        let resolved = resolve_uia2_window(measured_whatsapp_plan(), &windows)
            .expect("an invisible sibling must not refuse a resolvable WhatsApp");
        assert_eq!(resolved.bound_hwnd, MEASURED_RENDERER_HWND);
        assert_eq!(resolved.bound_process_id, MEASURED_WEBVIEW_PID);
    }

    /// An invisible SECOND qualifying host must not create ambiguity either --
    /// the same filter, checked from the other direction, so a future edit
    /// cannot satisfy one of these two tests by breaking the other.
    #[test]
    fn an_invisible_second_qualifying_host_does_not_make_the_graph_ambiguous() {
        let windows = measured_graph_plus_extra_host(
            730_001,
            730_002,
            80_001,
            MEASURED_SHELL_PID,
            Some("WhatsApp.Root.exe"),
            false,
            DECOY_AREA,
        );
        let resolved = resolve_uia2_window(measured_whatsapp_plan(), &windows)
            .expect("only one BINDABLE host qualifies, so there is no ambiguity");
        assert_eq!(resolved.bound_hwnd, MEASURED_RENDERER_HWND);
    }

    /// Mutant [4]: no parented host present. The refusal must still fire.
    ///
    /// This is the one-variable control for the whole change: the graph is the
    /// measured one with WhatsApp's WebView2 removed entirely, so the only
    /// WebView2 left is the decoy -- larger, visible, and the exact thing a
    /// fallback would grab.
    #[test]
    fn a_shell_with_no_parented_webview2_still_refuses() {
        let windows: Vec<_> = two_webview2_hosts_decoy_larger()
            .into_iter()
            .filter(|window| window.process_id != MEASURED_WEBVIEW_PID)
            .collect();

        assert!(
            windows
                .iter()
                .any(|window| window.process_id == DECOY_WEBVIEW_PID && window.visible),
            "the decoy must still be present, or the refusal is untested"
        );
        assert_eq!(
            resolve_uia2_window(measured_whatsapp_plan(), &windows),
            Err(Uia2WindowResolveError::MissingSiblingContentOuter),
            "a resolver that can no longer refuse is worse than one that cannot \
             resolve"
        );

        // And the refusal is one variable away from resolving: put WhatsApp's
        // own host back and the same plan, the same graph and the same decoy
        // produce a bind.
        let restored = two_webview2_hosts_decoy_larger();
        assert_eq!(
            resolve_uia2_window(measured_whatsapp_plan(), &restored)
                .expect("the full measured graph resolves")
                .bound_hwnd,
            MEASURED_RENDERER_HWND
        );
    }

    /// A pid of 0 is "the parentage was never read", and it must never match --
    /// including against a shell whose own process id somehow read as 0.
    #[test]
    fn an_unread_parentage_associates_with_nothing() {
        let shell = hosted_window(
            MEASURED_SHELL_HWND,
            None,
            0,
            0,
            None,
            MEASURED_SHELL_IMAGE,
            WHATSAPP_OUTER_WINDOW_CLASS,
            MEASURED_SHELL_AREA,
        );
        let host = hosted_window(
            MEASURED_WEBVIEW_OUTER_HWND,
            None,
            MEASURED_WEBVIEW_PID,
            0,
            Some("WhatsApp.Root.exe"),
            "msedgewebview2.exe",
            ELECTRON_OUTER_WINDOW_CLASS,
            MEASURED_WEBVIEW_AREA,
        );
        assert_eq!(
            classify_sibling_host(shell, host),
            SiblingHostAssociation::Foreign
        );
    }

    #[test]
    fn the_webview_exe_switch_is_read_off_the_measured_command_lines() {
        // Both forms as they appear on the owner's host.
        assert_eq!(
            webview2_host_exe_name(
                "\"C:\\Program Files (x86)\\Microsoft\\EdgeWebView\\Application\\msedgewebview2.exe\" \
                 --embedded-browser-webview=1 --webview-exe-name=WhatsApp.Root.exe \
                 --webview-exe-version=2.2629.100.0 --user-data-dir=C:\\Users\\o\\WebView2"
            ),
            Some("WhatsApp.Root.exe")
        );
        assert_eq!(
            webview2_host_exe_name("msedgewebview2.exe --webview-exe-name=SearchApp.exe --type=gpu"),
            Some("SearchApp.exe")
        );
        // Last argument, no trailing whitespace.
        assert_eq!(
            webview2_host_exe_name("msedgewebview2.exe --webview-exe-name=SearchApp.exe"),
            Some("SearchApp.exe")
        );
        // Quoted, because a hosting application's file name may contain spaces.
        assert_eq!(
            webview2_host_exe_name("msedgewebview2.exe --webview-exe-name=\"My App.exe\" --type=gpu"),
            Some("My App.exe")
        );
        // Absent and empty are both "no corroboration", never a match.
        assert_eq!(
            webview2_host_exe_name("msedgewebview2.exe --embedded-browser-webview=1"),
            None
        );
        assert_eq!(
            webview2_host_exe_name("msedgewebview2.exe --webview-exe-name= --type=gpu"),
            None
        );
    }

    /// The name trap, pinned. A-00b's fixture called the process `WhatsApp.exe`,
    /// so a `"WhatsApp"` match found nothing and the adapter reported "not
    /// running" on a machine where it was running.
    #[test]
    fn the_running_image_is_whatsapp_root_not_whatsapp() {
        assert_eq!(
            crate::native_whatsapp_adapter::WHATSAPP_DESKTOP_PROCESS_NAME,
            MEASURED_SHELL_PROCESS
        );
        assert!(same_process_name(MEASURED_SHELL_IMAGE, MEASURED_SHELL_PROCESS));
        assert!(
            !same_process_name(MEASURED_SHELL_IMAGE, "WhatsApp"),
            "`WhatsApp` matching `WhatsApp.Root.exe` is the trap, not the fix"
        );

        // The corroboration compares two OBSERVED names, both of which carry
        // `.exe`. `same_process_name` strips only one side, so using it here
        // classified WhatsApp's own WebView2 as `Contradicted` and refused.
        assert!(!same_process_name(MEASURED_SHELL_IMAGE, MEASURED_SHELL_IMAGE));
        assert!(same_image_name(MEASURED_SHELL_IMAGE, MEASURED_SHELL_IMAGE));
        assert!(same_image_name("WhatsApp.Root.EXE", "whatsapp.root.exe"));
        assert!(same_image_name("WhatsApp.Root", MEASURED_SHELL_IMAGE));
        assert!(!same_image_name("SearchApp.exe", MEASURED_SHELL_IMAGE));
    }

    /// The plan that ships is the plan this fixture resolves. Without this the
    /// tests above could be proving a plan nothing uses.
    #[test]
    fn the_shipping_whatsapp_plan_resolves_the_measured_graph() {
        let shipping = crate::native_whatsapp_adapter::WHATSAPP_UIA2_WINDOW_PLAN;
        assert_eq!(shipping.shape, Uia2WindowShape::SiblingChromiumRenderer);
        assert_eq!(shipping.app_process_name, MEASURED_SHELL_PROCESS);
        assert_eq!(shipping.sibling_process_name, Some(WEBVIEW2_PROCESS_NAME));

        let windows = two_webview2_hosts_decoy_larger();
        let resolved = resolve_uia2_window(shipping, &windows)
            .expect("the shipping plan must resolve the measured graph");
        assert_eq!(resolved.bound_hwnd, MEASURED_RENDERER_HWND);
        assert_eq!(resolved.bound_process_id, MEASURED_WEBVIEW_PID);
    }

    /// Telegram and Discord must not have moved. Neither goes near sibling
    /// association, and the new fields default to "never read", so the two
    /// shapes that do not consult them cannot have changed.
    #[test]
    fn parentage_does_not_reach_telegram_or_discord() {
        let telegram = Uia2WindowPlan::direct_outer_window(
            "Telegram",
            "Telegram",
            TELEGRAM_OUTER_WINDOW_CLASS,
            CALL_TIMEOUT_MS,
        );
        let host = RecordedHost::new(telegram_graph(), 743);
        let acquired = acquire_uia2_window(telegram, &host).expect("Telegram still acquires");
        assert_eq!(acquired.window.bound_hwnd, 0x2001);
        assert!(!acquired.woke);
        assert_eq!(telegram.tree_route, Uia2TreeRoute::UiaNative);

        assert!(
            telegram_graph()
                .iter()
                .chain(discord_graph().iter())
                .all(|window| window.parent_process_id == 0 && window.host_exe_name.is_none()),
            "neither provider's recorded graph reads the process table, so \
             neither can have been affected by a decision that consults it"
        );

        let discord = discord_plan();
        let discord_host = RecordedHost::new(discord_graph(), 696).chromium(2);
        let acquired =
            acquire_uia2_window(discord, &discord_host).expect("Discord still acquires");
        assert_eq!(acquired.window.bound_hwnd, 0x1001);
        assert_eq!(acquired.window.tree_route, Uia2TreeRoute::MsaaBridge);
    }

    /// Dump the sibling association on a live Windows host, read-only.
    ///
    /// This exists because the two facts D-156 turns on -- `ParentProcessId` and
    /// `--webview-exe-name` -- are read by `cfg(windows)` code that no Linux
    /// build compiles, so a green suite here says nothing about whether the
    /// producer can actually read them. It writes nothing, touches no composer,
    /// and needs neither a signed-in account nor an open conversation: the
    /// association is decided before any of that.
    ///
    /// ```text
    /// # from WSL, build the Windows test binary:
    /// flock -o /tmp/osl-cargo.lock cargo test --manifest-path apps/osl-hub/Cargo.toml \
    ///   --lib --target x86_64-pc-windows-gnu -j 4 --no-run
    /// # then on the Windows host, WhatsApp SHOWN (not closed to the tray):
    /// osl_privacy_hub-<hash>.exe --ignored --nocapture --test-threads=1 \
    ///   report_the_live_sibling_association
    /// ```
    ///
    /// Expected on the owner's host: WhatsApp's `msedgewebview2` prints
    /// `ppid=23884 host_exe=Some("WhatsApp.Root.exe") assoc=OwnedByApp`, Windows
    /// Search's prints `assoc=Foreign`, and the resolve line reports the
    /// WhatsApp-parented pid. An `assoc=Uncorroborated` on WhatsApp's own host
    /// means the command-line read was refused, and the refusal that follows is
    /// then correct rather than a mis-bind.
    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "reads the live process table on a Windows host; run explicitly"]
    fn report_the_live_sibling_association() {
        use crate::native_a11y::win32::Uia2Win32Host;

        let plan = crate::native_whatsapp_adapter::WHATSAPP_UIA2_WINDOW_PLAN;
        let host = Uia2Win32Host::desktop();
        let windows = host
            .enumerate_windows(Uia2Deadline::from_plan(plan))
            .expect("the desktop enumeration must answer inside the plan's budget");
        let candidates: Vec<_> = windows.iter().map(Uia2OwnedWindow::candidate).collect();

        let app_outer = candidates
            .iter()
            .copied()
            .filter(|window| {
                same_process_name(window.process_name, plan.app_process_name)
                    && window.class_name == plan.app_outer_class
                    && window.visible
            })
            .max_by_key(|window| window.area);

        for window in candidates.iter().copied().filter(|window| {
            let name = window.process_name.to_ascii_lowercase();
            name.contains("whatsapp") || name.contains("webview2")
        }) {
            let association = app_outer
                .map(|app_outer| format!("{:?}", classify_sibling_host(app_outer, window)))
                .unwrap_or_else(|| "no-shell".to_owned());
            eprintln!(
                "  candidate hwnd={} pid={} ppid={} image={:?} host_exe={:?} class={:?} \
                 visible={} area={} parent={:?} associated_app={:?} assoc={association}",
                window.hwnd,
                window.process_id,
                window.parent_process_id,
                window.process_name,
                window.host_exe_name,
                window.class_name,
                window.visible,
                window.area,
                window.parent_hwnd,
                window.associated_app_hwnd,
            );
        }

        match resolve_uia2_window(plan, &candidates) {
            Ok(resolved) => eprintln!(
                "whatsapp: RESOLVED bound_pid={} bound_is_app_shell={}",
                resolved.bound_process_id,
                resolved.bound_hwnd == resolved.app_outer_hwnd
            ),
            Err(error) => eprintln!("whatsapp: REFUSED {error:?}"),
        }
    }

    #[test]
    fn sibling_chromium_shape_rejects_app_root_only() {
        let plan = Uia2WindowPlan::sibling_chromium_renderer(
            "WhatsApp",
            "WhatsApp",
            WHATSAPP_OUTER_WINDOW_CLASS,
            WEBVIEW2_PROCESS_NAME,
            WAIT_MS,
            CALL_TIMEOUT_MS,
        );
        let windows = [window(
            30,
            None,
            None,
            300,
            "WhatsApp",
            WHATSAPP_OUTER_WINDOW_CLASS,
            900,
        )];

        assert_eq!(
            resolve_uia2_window(plan, &windows),
            Err(Uia2WindowResolveError::MissingSiblingContentOuter)
        );
    }
}
