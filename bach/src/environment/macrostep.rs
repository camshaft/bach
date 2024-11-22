#[derive(Clone, Copy, Debug, Default)]
pub struct Macrostep {
    pub tasks: usize,
    pub ticks: u64,
}

impl Macrostep {
    pub fn metrics(&self) {
        measure!("tasks", self.tasks as u32);
        measure!(
            "advance",
            crate::time::resolution::ticks_to_duration(self.ticks)
        );
    }
}
