//! The Update button at the top of the window: Chatter asks GitHub for a
//! newer release when it starts and every six hours, and installs one when
//! the button is pressed, or right away at start with Update automatically on.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

use crate::updater::{self, Release};

use super::*;

/// How often GitHub is asked again while Chatter runs.
const EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// How often an answer is looked for.
const POLL: Duration = Duration::from_millis(500);

/// The first check since Chatter started is done; only it installs by itself.
static STARTED: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
pub(super) struct Updates {
    /// The newer release found, which the button installs.
    release: Option<Release>,
    timer: Option<Rc<slint::Timer>>,
}

type State = std::rc::Weak<RefCell<Adapter>>;

/// Waits for a background job's answer without holding up the window.
fn wait<T: 'static>(answer: mpsc::Receiver<T>, done: impl FnOnce(T) + 'static) {
    slint::Timer::single_shot(POLL, move || match answer.try_recv() {
        Ok(value) => done(value),
        Err(mpsc::TryRecvError::Empty) => wait(answer, done),
        Err(mpsc::TryRecvError::Disconnected) => {}
    });
}

fn check(state: State, weak: slint::Weak<ChatterWindow>) {
    let (sent, answer) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sent.send(updater::check());
    });
    wait(answer, move |found| {
        let first = !STARTED.swap(true, Ordering::Relaxed);
        let release = match found {
            Ok(Some(release)) => release,
            Ok(None) => return,
            Err(error) => {
                tracing::info!(%error, "could not check for updates");
                return;
            }
        };
        let (Some(adapter), Some(window)) = (state.upgrade(), weak.upgrade()) else {
            return;
        };
        window.set_update_version(release.version.as_str().into());
        let automatic = adapter.borrow().layout.auto_update.unwrap_or(true);
        adapter.borrow_mut().updates.release = Some(release);
        if first && automatic && cfg!(windows) {
            install(&adapter, &window);
        }
    });
}

/// Downloads and installs the release found, then quits so it can start.
fn install(adapter: &Rc<RefCell<Adapter>>, window: &ChatterWindow) {
    let Some(release) = adapter.borrow().updates.release.clone() else {
        return;
    };
    if window.get_update_status() == "Updating..." {
        return;
    }
    window.set_update_status("Updating...".into());
    window.set_update_progress(SharedString::default());
    // How much has arrived, under the bar, while it downloads.
    let total = release.size;
    let ticker = Rc::new(slint::Timer::default());
    {
        let weak = window.as_weak();
        ticker.start(slint::TimerMode::Repeated, Duration::from_millis(300), move || {
            let Some(window) = weak.upgrade() else { return };
            let mb = |bytes: u64| bytes as f64 / 1_048_576.0;
            let got = updater::downloaded();
            window.set_update_progress(
                match total {
                    Some(total) if total > 0 => format!(
                        "{:.1} of {:.1} MB ({}%)",
                        mb(got),
                        mb(total),
                        (got * 100 / total).min(100)
                    ),
                    _ => format!("{:.1} MB downloaded", mb(got)),
                }
                .into(),
            );
        });
    }
    let (sent, answer) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sent.send(updater::install(&release));
    });
    let weak = window.as_weak();
    let state = Rc::downgrade(adapter);
    wait(answer, move |installed| match (drop(ticker), installed).1 {
        Ok(()) if cfg!(windows) => {
            // Straight out, so the new version starts within a second or two:
            // a normal quit waits a minute for Discord's connections and
            // background work to wind down.
            if let Some(adapter) = state.upgrade() {
                let mut adapter = adapter.borrow_mut();
                super::placement::flush(&adapter);
                adapter.tray = None;
            }
            std::process::exit(0);
        }
        Ok(()) => {
            if let Some(window) = weak.upgrade() {
                window.set_update_status(SharedString::default());
            }
        }
        Err(error) if error == "cancelled" => {
            if let Some(window) = weak.upgrade() {
                window.set_update_status(SharedString::default());
            }
        }
        Err(error) => {
            tracing::warn!(%error, "could not update");
            if let Some(window) = weak.upgrade() {
                window.set_update_status(format!("Update failed: {error}").into());
            }
        }
    });
}

pub(super) fn bind(window: &ChatterWindow, adapter: &Rc<RefCell<Adapter>>) {
    window.on_cancel_update(updater::cancel);
    {
        let (state, weak) = (Rc::downgrade(adapter), window.as_weak());
        window.on_install_update(move || {
            if let (Some(adapter), Some(window)) = (state.upgrade(), weak.upgrade()) {
                install(&adapter, &window);
            }
        });
    }
    // The card asking to join the Chatter Discord shows once, signed in.
    window.set_community_card(!adapter.borrow().layout.community_asked && !cfg!(test));
    {
        let (state, weak) = (Rc::downgrade(adapter), window.as_weak());
        window.on_join_community(move |code| {
            if let (Some(adapter), Some(window)) = (state.upgrade(), weak.upgrade()) {
                let mut adapter = adapter.borrow_mut();
                adapter.layout.community_asked = true;
                adapter.layout.write(&adapter.root);
                adapter.action(&window, chatter_core::application::Action::OpenInviteLink(code.into()));
            }
        });
    }
    {
        let state = Rc::downgrade(adapter);
        window.on_community_dismissed(move || {
            if let Some(adapter) = state.upgrade() {
                let mut adapter = adapter.borrow_mut();
                adapter.layout.community_asked = true;
                adapter.layout.write(&adapter.root);
            }
        });
    }
    {
        let state = Rc::downgrade(adapter);
        window.on_auto_update_toggled(move |on| {
            if let Some(adapter) = state.upgrade() {
                let mut adapter = adapter.borrow_mut();
                adapter.layout.auto_update = Some(on);
                adapter.layout.write(&adapter.root);
            }
        });
    }
    // A window made again keeps what was found.
    if let Some(release) = &adapter.borrow().updates.release {
        window.set_update_version(release.version.as_str().into());
    }
    // Tests make many windows; asking GitHub for each would use up its
    // hourly limit for this computer.
    if cfg!(test) {
        return;
    }
    if !STARTED.load(Ordering::Relaxed) {
        std::thread::spawn(updater::clean_up);
    }
    check(Rc::downgrade(adapter), window.as_weak());
    let timer = Rc::new(slint::Timer::default());
    let (state, weak) = (Rc::downgrade(adapter), window.as_weak());
    timer.start(slint::TimerMode::Repeated, EVERY, move || {
        check(state.clone(), weak.clone());
    });
    adapter.borrow_mut().updates.timer = Some(timer);
}
