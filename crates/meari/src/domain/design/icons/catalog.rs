//! Proposed product roles. No geometry is implicitly downloaded or substituted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum IconRole {
    WindowMinimize,
    WindowMaximize,
    WindowRestore,
    WindowClose,
    MediaPlay,
    MediaPause,
    MediaPrevious,
    MediaNext,
    NavigationBack,
    ActionOpenFile,
    ActionSearch,
    StateInfo,
    StateWarning,
}
pub struct RoleMapping {
    pub role: IconRole,
    pub semantic_name: &'static str,
    pub upstream_name: &'static str,
    pub react_export: &'static str,
}
pub const MAPPINGS: &[RoleMapping] = &[
    RoleMapping {
        role: IconRole::WindowMinimize,
        semantic_name: "window.minimize",
        upstream_name: "minus",
        react_export: "Minus",
    },
    RoleMapping {
        role: IconRole::WindowMaximize,
        semantic_name: "window.maximize",
        upstream_name: "square",
        react_export: "Square",
    },
    RoleMapping {
        role: IconRole::WindowRestore,
        semantic_name: "window.restore",
        upstream_name: "copy",
        react_export: "Copy",
    },
    RoleMapping {
        role: IconRole::WindowClose,
        semantic_name: "window.close",
        upstream_name: "x",
        react_export: "X",
    },
    RoleMapping {
        role: IconRole::MediaPlay,
        semantic_name: "media.play",
        upstream_name: "play",
        react_export: "Play",
    },
    RoleMapping {
        role: IconRole::MediaPause,
        semantic_name: "media.pause",
        upstream_name: "pause",
        react_export: "Pause",
    },
    RoleMapping {
        role: IconRole::MediaPrevious,
        semantic_name: "media.previous",
        upstream_name: "skip-back",
        react_export: "SkipBack",
    },
    RoleMapping {
        role: IconRole::MediaNext,
        semantic_name: "media.next",
        upstream_name: "skip-forward",
        react_export: "SkipForward",
    },
    RoleMapping {
        role: IconRole::NavigationBack,
        semantic_name: "navigation.back",
        upstream_name: "arrow-left",
        react_export: "ArrowLeft",
    },
    RoleMapping {
        role: IconRole::ActionOpenFile,
        semantic_name: "action.open-file",
        upstream_name: "folder-open",
        react_export: "FolderOpen",
    },
    RoleMapping {
        role: IconRole::ActionSearch,
        semantic_name: "action.search",
        upstream_name: "search",
        react_export: "Search",
    },
    RoleMapping {
        role: IconRole::StateInfo,
        semantic_name: "state.info",
        upstream_name: "info",
        react_export: "Info",
    },
    RoleMapping {
        role: IconRole::StateWarning,
        semantic_name: "state.warning",
        upstream_name: "triangle-alert",
        react_export: "TriangleAlert",
    },
];
impl IconRole {
    pub fn mapping(self) -> &'static RoleMapping {
        MAPPINGS
            .iter()
            .find(|mapping| mapping.role == self)
            .expect("closed role mapping")
    }
}

/// Official React/static metadata pin, not a claim of an official Rust package.
pub struct UpstreamPin {
    pub repository: &'static str,
    pub version: &'static str,
    pub revision: &'static str,
    pub react_package: &'static str,
    pub static_package: &'static str,
    pub react_integrity: &'static str,
    pub static_integrity: &'static str,
    pub license_sha256: &'static str,
}
pub const LUCIDE: UpstreamPin = UpstreamPin {
    repository: "https://github.com/lucide-icons/lucide",
    version: "1.61.0",
    revision: "c29c276f16590cf017126671535ad8c792be64d8",
    react_package: "lucide-react",
    static_package: "lucide-static",
    react_integrity: "sha512-v2SQPPPNjnr+BlhL9Spuzmo5yR+hLkftfuE4E3jaxdNPgSH0hJKPcLJBP7yAta87jQNPsSERS59PJjCJ/CorBA==",
    static_integrity: "sha512-yLAiQB3MWUJEstsEDAwenSA3m7vr1tdLvgnC16Q/XMxRRma0DWwMs8M2bWlvgShrIs1m2mWMJ+0AiahqQO4GrA==",
    license_sha256: "b495047bd93a9b06913511076f504daba17d5bbeb3e0650f3bb53a4220329c57",
};
