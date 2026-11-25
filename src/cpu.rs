//! CPU core frequencies, sampled about every 100 ms while a benchmark run executes.

use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use sysinfo::System;

use crate::benchmark::{CpuMonitor, CpuRecording, CpuSample};

const INTERVAL: Duration = Duration::from_millis(100);

pub struct SystemCpuMonitor;

impl CpuMonitor for SystemCpuMonitor {
    fn start(&self) -> Box<dyn CpuRecording> {
        let (stop, stopped) = mpsc::channel::<()>();
        let sampler = std::thread::spawn(move || {
            let mut system = match System::new() {
                Ok(system) => system,
                Err(error) => {
                    tracing::warn!(
                        "cannot read CPU frequencies ({error}); this run has no cpu_freq.csv rows"
                    );
                    return Vec::new();
                }
            };
            let start = Instant::now();
            let mut samples = Vec::new();
            loop {
                system.refresh_cpu_frequency();
                let ms_since_start = start.elapsed().as_millis() as u64;
                samples.extend(system.cpus().iter().enumerate().map(|(core_index, cpu)| {
                    CpuSample {
                        core_index,
                        mhz: cpu.frequency(),
                        ms_since_start,
                    }
                }));
                // The interval is the sampling rate the contract asks for, and stop ends it early.
                if stopped.recv_timeout(INTERVAL) != Err(mpsc::RecvTimeoutError::Timeout) {
                    return samples;
                }
            }
        });
        Box::new(Recording { stop, sampler })
    }
}

struct Recording {
    stop: mpsc::Sender<()>,
    sampler: JoinHandle<Vec<CpuSample>>,
}

impl CpuRecording for Recording {
    fn stop(self: Box<Self>) -> Vec<CpuSample> {
        // A sampler that already ended has dropped its receiver; its samples are still joined.
        let _ = self.stop.send(());
        self.sampler.join().expect("CPU sampler panicked")
    }
}
