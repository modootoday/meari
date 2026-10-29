//! One demo session: the static server plus the current pair of windows.
use crate::catalog::Prepared;
use crate::launch::{native, process::Owned, server::Server, web};
use crate::screen::{layout, x11};
use std::path::PathBuf;
use std::time::Duration;

pub struct Session {
    server: Server,
    chrome: String,
    profile: PathBuf,
    panes: layout::Panes,
    windows: Vec<Owned>,
}

impl Session {
    pub fn start(root: &std::path::Path, chrome: String) -> Result<Self, String> {
        let (w, h) = x11::display_size();
        Ok(Self {
            server: Server::start(root)?,
            chrome,
            profile: root.join(".chrome-profile"),
            panes: layout::split(w, h),
            windows: Vec::new(),
        })
    }

    /// Replace both windows with the given fixture.
    pub fn show(&mut self, prepared: &Prepared) -> Result<(), String> {
        self.windows.clear(); // Drop stops the previous pair
        let url = self.server.url(&prepared.page);
        self.windows
            .push(native::spawn(prepared, self.panes.native)?);
        self.windows.push(web::spawn(
            &self.chrome,
            &url,
            self.panes.web,
            &self.profile,
        )?);
        let placed = x11::place(
            &native::title(&prepared.name),
            self.panes.native,
            Duration::from_secs(20),
        );
        println!(
            "showing {} | native {:?} | web {url} {:?}",
            prepared.name, self.panes.native, self.panes.web
        );
        placed
    }

    /// Labels of windows that were closed by the user or crashed.
    pub fn exited(&mut self) -> Vec<&'static str> {
        self.windows
            .iter_mut()
            .filter_map(|w| w.exited().then_some(w.label))
            .collect()
    }
}
