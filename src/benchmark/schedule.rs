//! The order in which benchmark runs execute.

use crate::config::RunOrder;

/// One run to execute: the save (an index into the selected saves) and its n-th run in this
/// command, counted from 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ScheduledRun {
    pub save: usize,
    pub nth: u32,
}

pub fn schedule(
    saves: usize,
    runs: u32,
    order: RunOrder,
    rng: &mut fastrand::Rng,
) -> Vec<ScheduledRun> {
    let grouped = (0..saves).flat_map(|save| (0..runs).map(move |nth| ScheduledRun { save, nth }));
    match order {
        RunOrder::Grouped => grouped.collect(),
        RunOrder::Sequential => (0..runs)
            .flat_map(|nth| (0..saves).map(move |save| ScheduledRun { save, nth }))
            .collect(),
        RunOrder::Random => {
            let mut runs: Vec<ScheduledRun> = grouped.collect();
            rng.shuffle(&mut runs);
            runs
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saves_of(runs: &[ScheduledRun]) -> Vec<usize> {
        runs.iter().map(|run| run.save).collect()
    }

    #[test]
    fn grouped_runs_every_run_of_a_save_before_the_next_save() {
        let runs = schedule(2, 3, RunOrder::Grouped, &mut fastrand::Rng::with_seed(1));

        assert_eq!(saves_of(&runs), vec![0, 0, 0, 1, 1, 1]);
    }

    #[test]
    fn sequential_alternates_between_saves() {
        let runs = schedule(2, 3, RunOrder::Sequential, &mut fastrand::Rng::with_seed(1));

        assert_eq!(saves_of(&runs), vec![0, 1, 0, 1, 0, 1]);
    }

    #[test]
    fn random_runs_every_scheduled_run_exactly_once() {
        // Protects the decision that `random` shuffles the whole schedule as one pool, so every
        // save keeps exactly `runs` runs (indexes 0..runs), rather than drawing a save per slot.
        let mut runs = schedule(3, 4, RunOrder::Random, &mut fastrand::Rng::with_seed(7));
        runs.sort();

        assert_eq!(
            runs,
            schedule(3, 4, RunOrder::Grouped, &mut fastrand::Rng::with_seed(7))
        );
    }
}
