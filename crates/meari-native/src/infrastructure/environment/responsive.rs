use super::super::engine::NativeRenderer;
use meari::{application::responsive::ResponsiveRenderer, domain::host::HostEnvironment};

impl ResponsiveRenderer for NativeRenderer {
    fn apply_environment(&mut self, environment: &HostEnvironment) -> Result<(), String> {
        self.layout_environment(environment)
    }
}
