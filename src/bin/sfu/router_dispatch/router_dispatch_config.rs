pub struct RouterDispatchConfig {
	pub max_workers: usize,
	pub consumers_per_worker: u32,
}

impl Default for RouterDispatchConfig {
	fn default() -> Self {
		RouterDispatchConfig {
			max_workers: 1,
			consumers_per_worker: 500,
		}
	}
}
