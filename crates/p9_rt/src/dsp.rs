#[derive(Clone, Copy, Debug)]
pub struct DspBudget {
    pub max_block_us: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DspFrameStats {
    pub block_us: u32,
    pub xrun: bool,
}

pub struct DspPipeline {
    budget: DspBudget,
    last_stats: DspFrameStats,
}

impl DspPipeline {
    pub fn new(max_block_us: u32) -> Self {
        Self {
            budget: DspBudget { max_block_us },
            last_stats: DspFrameStats::default(),
        }
    }

    pub fn process_block(&mut self, simulated_block_us: u32) -> DspFrameStats {
        self.last_stats = DspFrameStats {
            block_us: simulated_block_us,
            xrun: simulated_block_us > self.budget.max_block_us,
        };
        self.last_stats
    }

    pub fn last_stats(&self) -> DspFrameStats {
        self.last_stats
    }
}
