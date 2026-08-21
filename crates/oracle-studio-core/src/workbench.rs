use std::collections::BTreeSet;

use chrono::{
    DateTime, Datelike, Duration, LocalResult, NaiveDate, NaiveDateTime, Offset, TimeZone,
    Timelike, Utc,
};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

use crate::{AmbiguousTimeChoice, LocalDateTimeInput, ModelError, StableId};

/// Keeps rapid preview input behind a single in-flight worker request.
///
/// The UI owns the payload, while this state machine owns generation ordering:
/// one request is dispatched immediately, subsequent input replaces the one
/// queued request, and only the newest generation may update the presentation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewCoordinator<T> {
    newest_generation: u64,
    in_flight: Option<u64>,
    queued: Option<(u64, T)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreviewEnqueue<T> {
    Dispatch { generation: u64, payload: T },
    Coalesced { generation: u64 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewCompletion<T> {
    pub accept_response: bool,
    pub next: Option<(u64, T)>,
}

impl<T> Default for PreviewCoordinator<T> {
    fn default() -> Self {
        Self {
            newest_generation: 0,
            in_flight: None,
            queued: None,
        }
    }
}

impl<T> PreviewCoordinator<T> {
    pub fn enqueue(&mut self, payload: T) -> PreviewEnqueue<T> {
        self.newest_generation = self.newest_generation.saturating_add(1);
        let generation = self.newest_generation;
        if self.in_flight.is_some() {
            self.queued = Some((generation, payload));
            PreviewEnqueue::Coalesced { generation }
        } else {
            self.in_flight = Some(generation);
            PreviewEnqueue::Dispatch {
                generation,
                payload,
            }
        }
    }

    pub fn complete(&mut self, generation: u64) -> PreviewCompletion<T> {
        if self.in_flight != Some(generation) {
            return PreviewCompletion {
                accept_response: false,
                next: None,
            };
        }
        self.in_flight = None;
        let next = self.queued.take();
        if let Some((next_generation, _)) = next.as_ref() {
            self.in_flight = Some(*next_generation);
        }
        PreviewCompletion {
            accept_response: generation == self.newest_generation,
            next,
        }
    }

    pub fn cancel(&mut self) {
        self.in_flight = None;
        self.queued = None;
    }

    pub const fn newest_generation(&self) -> u64 {
        self.newest_generation
    }
}

/// One of the exact workbench time-controller columns.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeInterval {
    Minute,
    TenMinutes,
    Hour,
    Day,
    FiveDays,
    ThirtyDays,
    Year,
    TenYears,
}

impl TimeInterval {
    pub const ALL: [Self; 8] = [
        Self::Minute,
        Self::TenMinutes,
        Self::Hour,
        Self::Day,
        Self::FiveDays,
        Self::ThirtyDays,
        Self::Year,
        Self::TenYears,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Minute => "1m",
            Self::TenMinutes => "10m",
            Self::Hour => "1h",
            Self::Day => "1d",
            Self::FiveDays => "5d",
            Self::ThirtyDays => "30d",
            Self::Year => "1y",
            Self::TenYears => "10y",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepDirection {
    Backward,
    Forward,
}

impl StepDirection {
    const fn sign(self) -> i64 {
        match self {
            Self::Backward => -1,
            Self::Forward => 1,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimeStepResult {
    pub local_input: LocalDateTimeInput,
    pub utc_instant: String,
    pub utc_offset_seconds: i32,
    pub ambiguous_time_choice: Option<AmbiguousTimeChoice>,
    pub adjustment_notice: Option<String>,
}

/// Move a civil-time cursor according to the workbench controller contract.
pub fn step_local_time(
    input: &LocalDateTimeInput,
    previous_utc_offset_seconds: Option<i32>,
    interval: TimeInterval,
    direction: StepDirection,
) -> Result<TimeStepResult, ModelError> {
    let zone = input
        .time_zone()
        .parse::<Tz>()
        .map_err(|_| ModelError::InvalidValue("chart.time_zone"))?;
    let current_naive = naive(input)?;
    let current = choose_local(
        &zone,
        current_naive,
        previous_utc_offset_seconds,
        direction,
        None,
    )
    .ok_or(ModelError::NonexistentLocalTime)?;

    match interval {
        TimeInterval::Minute | TimeInterval::TenMinutes | TimeInterval::Hour => {
            let minutes = match interval {
                TimeInterval::Minute => 1,
                TimeInterval::TenMinutes => 10,
                TimeInterval::Hour => 60,
                _ => unreachable!(),
            } * direction.sign();
            let moved = current.with_timezone(&Utc) + Duration::minutes(minutes);
            Ok(result(moved.with_timezone(&zone), None)?)
        }
        TimeInterval::Day | TimeInterval::FiveDays | TimeInterval::ThirtyDays => {
            let days = match interval {
                TimeInterval::Day => 1,
                TimeInterval::FiveDays => 5,
                TimeInterval::ThirtyDays => 30,
                _ => unreachable!(),
            } * direction.sign();
            let target = current_naive
                .checked_add_signed(Duration::days(days))
                .ok_or(ModelError::InvalidValue("workbench.time_cursor"))?;
            resolve_wall_clock_target(
                &zone,
                target,
                current,
                previous_utc_offset_seconds,
                direction,
            )
        }
        TimeInterval::Year | TimeInterval::TenYears => {
            let years = match interval {
                TimeInterval::Year => 1,
                TimeInterval::TenYears => 10,
                _ => unreachable!(),
            } * direction.sign() as i32;
            let year = current_naive
                .year()
                .checked_add(years)
                .ok_or(ModelError::InvalidValue("workbench.time_cursor"))?;
            let date = NaiveDate::from_ymd_opt(year, current_naive.month(), current_naive.day())
                .or_else(|| {
                    (current_naive.month() == 2 && current_naive.day() == 29)
                        .then(|| NaiveDate::from_ymd_opt(year, 2, 28))
                        .flatten()
                })
                .ok_or(ModelError::InvalidValue("workbench.time_cursor"))?;
            resolve_wall_clock_target(
                &zone,
                date.and_time(current_naive.time()),
                current,
                previous_utc_offset_seconds,
                direction,
            )
        }
    }
}

fn resolve_wall_clock_target(
    zone: &Tz,
    target: NaiveDateTime,
    current: DateTime<Tz>,
    previous_offset: Option<i32>,
    direction: StepDirection,
) -> Result<TimeStepResult, ModelError> {
    if let Some(value) = choose_local(
        zone,
        target,
        previous_offset,
        direction,
        Some(current.with_timezone(&Utc)),
    ) {
        return result(value, None);
    }

    for minute in 1..=360 {
        let candidate = target
            .checked_add_signed(Duration::minutes(direction.sign() * minute))
            .ok_or(ModelError::InvalidValue("workbench.time_cursor"))?;
        if let Some(value) = choose_local(
            zone,
            candidate,
            previous_offset,
            direction,
            Some(current.with_timezone(&Utc)),
        ) {
            return result(
                value,
                Some(format!(
                    "{} does not exist in {}; adjusted {} to the nearest valid local time",
                    target.format("%Y-%m-%d %H:%M:%S"),
                    zone,
                    match direction {
                        StepDirection::Forward => "forward",
                        StepDirection::Backward => "backward",
                    }
                )),
            );
        }
    }
    Err(ModelError::NonexistentLocalTime)
}

fn choose_local(
    zone: &Tz,
    target: NaiveDateTime,
    preferred_offset: Option<i32>,
    direction: StepDirection,
    current_utc: Option<DateTime<Utc>>,
) -> Option<DateTime<Tz>> {
    match zone.from_local_datetime(&target) {
        LocalResult::Single(value) => Some(value),
        LocalResult::None => None,
        LocalResult::Ambiguous(first, second) => {
            let mut candidates = [first, second];
            candidates.sort_by_key(|candidate| candidate.with_timezone(&Utc));
            if let Some(offset) = preferred_offset
                && let Some(candidate) = candidates
                    .iter()
                    .find(|candidate| candidate.offset().fix().local_minus_utc() == offset)
            {
                return Some(*candidate);
            }
            if let Some(current) = current_utc {
                match direction {
                    StepDirection::Forward => candidates
                        .into_iter()
                        .filter(|candidate| candidate.with_timezone(&Utc) > current)
                        .min_by_key(|candidate| candidate.with_timezone(&Utc))
                        .or(Some(candidates[0])),
                    StepDirection::Backward => candidates
                        .into_iter()
                        .filter(|candidate| candidate.with_timezone(&Utc) < current)
                        .max_by_key(|candidate| candidate.with_timezone(&Utc))
                        .or(Some(candidates[1])),
                }
            } else {
                Some(match direction {
                    StepDirection::Forward => candidates[0],
                    StepDirection::Backward => candidates[1],
                })
            }
        }
    }
}

fn naive(input: &LocalDateTimeInput) -> Result<NaiveDateTime, ModelError> {
    let date = NaiveDate::parse_from_str(input.local_date(), "%Y-%m-%d")
        .map_err(|_| ModelError::InvalidValue("chart.local_date"))?;
    let time = chrono::NaiveTime::parse_from_str(input.local_time(), "%H:%M:%S")
        .map_err(|_| ModelError::InvalidValue("chart.local_time"))?;
    Ok(date.and_time(time))
}

fn result(
    value: DateTime<Tz>,
    adjustment_notice: Option<String>,
) -> Result<TimeStepResult, ModelError> {
    let ambiguous_time_choice = match value.timezone().from_local_datetime(&value.naive_local()) {
        LocalResult::Ambiguous(first, second) => {
            let mut candidates = [first, second];
            candidates.sort_by_key(|candidate| candidate.with_timezone(&Utc));
            Some(
                if value.with_timezone(&Utc) == candidates[0].with_timezone(&Utc) {
                    AmbiguousTimeChoice::Earlier
                } else {
                    AmbiguousTimeChoice::Later
                },
            )
        }
        LocalResult::Single(_) | LocalResult::None => None,
    };
    Ok(TimeStepResult {
        local_input: LocalDateTimeInput::new(
            value.date_naive().format("%Y-%m-%d").to_string(),
            format!(
                "{:02}:{:02}:{:02}",
                value.hour(),
                value.minute(),
                value.second()
            ),
            value.timezone().to_string(),
        )?,
        utc_instant: value.with_timezone(&Utc).to_rfc3339(),
        utc_offset_seconds: value.offset().fix().local_minus_utc(),
        ambiguous_time_choice,
        adjustment_notice,
    })
}

/// Generate a stable lowercase hyphenated ID without exposing an ID form field.
pub fn generate_unique_id(
    entity_type: &str,
    display_name: &str,
    existing: &BTreeSet<String>,
) -> Result<StableId, ModelError> {
    let fallback = slug(entity_type);
    let base = match slug(display_name) {
        value if !value.is_empty() => value,
        _ if !fallback.is_empty() => fallback,
        _ => "item".into(),
    };
    let mut candidate = base.clone();
    let mut suffix = 2_u32;
    while existing.contains(&candidate) {
        candidate = format!("{base}-{suffix}");
        suffix = suffix
            .checked_add(1)
            .ok_or(ModelError::InvalidValue("generated.id"))?;
    }
    StableId::new("generated.id", candidate)
}

fn slug(value: &str) -> String {
    let mut result = String::new();
    let mut separator = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            if separator && !result.is_empty() {
                result.push('-');
            }
            result.push(character.to_ascii_lowercase());
            separator = false;
        } else if !result.is_empty() {
            separator = true;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(date: &str, time: &str, zone: &str) -> LocalDateTimeInput {
        LocalDateTimeInput::new(date, time, zone).unwrap()
    }

    #[test]
    fn elapsed_intervals_cross_dst_by_elapsed_time() {
        let start = input("2026-03-08", "01:30:00", "America/New_York");
        let hour = step_local_time(
            &start,
            Some(-18_000),
            TimeInterval::Hour,
            StepDirection::Forward,
        )
        .unwrap();
        assert_eq!(hour.local_input.local_time(), "03:30:00");
        assert_eq!(hour.utc_offset_seconds, -14_400);
        let ten = step_local_time(
            &start,
            Some(-18_000),
            TimeInterval::TenMinutes,
            StepDirection::Backward,
        )
        .unwrap();
        assert_eq!(ten.local_input.local_time(), "01:20:00");
    }

    #[test]
    fn every_wall_clock_interval_moves_both_directions() {
        let start = input("2026-01-15", "12:34:56", "America/New_York");
        for (interval, forward, backward) in [
            (
                TimeInterval::Minute,
                "2026-01-15 12:35:56",
                "2026-01-15 12:33:56",
            ),
            (
                TimeInterval::TenMinutes,
                "2026-01-15 12:44:56",
                "2026-01-15 12:24:56",
            ),
            (
                TimeInterval::Hour,
                "2026-01-15 13:34:56",
                "2026-01-15 11:34:56",
            ),
            (
                TimeInterval::Day,
                "2026-01-16 12:34:56",
                "2026-01-14 12:34:56",
            ),
            (
                TimeInterval::FiveDays,
                "2026-01-20 12:34:56",
                "2026-01-10 12:34:56",
            ),
            (
                TimeInterval::ThirtyDays,
                "2026-02-14 12:34:56",
                "2025-12-16 12:34:56",
            ),
            (
                TimeInterval::Year,
                "2027-01-15 12:34:56",
                "2025-01-15 12:34:56",
            ),
            (
                TimeInterval::TenYears,
                "2036-01-15 12:34:56",
                "2016-01-15 12:34:56",
            ),
        ] {
            let next =
                step_local_time(&start, Some(-18_000), interval, StepDirection::Forward).unwrap();
            let previous =
                step_local_time(&start, Some(-18_000), interval, StepDirection::Backward).unwrap();
            assert_eq!(
                format!(
                    "{} {}",
                    next.local_input.local_date(),
                    next.local_input.local_time()
                ),
                forward
            );
            assert_eq!(
                format!(
                    "{} {}",
                    previous.local_input.local_date(),
                    previous.local_input.local_time()
                ),
                backward
            );
        }
    }

    #[test]
    fn wall_clock_steps_adjust_gaps_and_preserve_overlap_offset() {
        let gap = input("2026-03-07", "02:30:00", "America/New_York");
        let adjusted = step_local_time(
            &gap,
            Some(-18_000),
            TimeInterval::Day,
            StepDirection::Forward,
        )
        .unwrap();
        assert_eq!(adjusted.local_input.local_time(), "03:00:00");
        assert!(adjusted.adjustment_notice.is_some());

        let overlap = input("2026-10-02", "01:30:00", "America/New_York");
        let retained = step_local_time(
            &overlap,
            Some(-18_000),
            TimeInterval::ThirtyDays,
            StepDirection::Forward,
        )
        .unwrap();
        assert_eq!(retained.utc_offset_seconds, -18_000);
        assert_eq!(
            retained.ambiguous_time_choice,
            Some(AmbiguousTimeChoice::Later)
        );
    }

    #[test]
    fn year_steps_clamp_february_29() {
        let leap = input("2024-02-29", "08:15:00", "UTC");
        let next =
            step_local_time(&leap, Some(0), TimeInterval::Year, StepDirection::Forward).unwrap();
        assert_eq!(next.local_input.local_date(), "2025-02-28");
        assert_eq!(next.local_input.local_time(), "08:15:00");
    }

    #[test]
    fn ids_slug_collide_and_fall_back_without_changing_existing_ids() {
        let existing = BTreeSet::from(["alice-example".into(), "alice-example-2".into()]);
        assert_eq!(
            generate_unique_id("person", "Alice Example", &existing)
                .unwrap()
                .as_str(),
            "alice-example-3"
        );
        assert_eq!(
            generate_unique_id("wheel-template", "星", &BTreeSet::new())
                .unwrap()
                .as_str(),
            "wheel-template"
        );
    }

    #[test]
    fn preview_requests_coalesce_and_stale_responses_are_discarded() {
        let mut coordinator = PreviewCoordinator::default();
        assert_eq!(
            coordinator.enqueue("first"),
            PreviewEnqueue::Dispatch {
                generation: 1,
                payload: "first"
            }
        );
        assert_eq!(
            coordinator.enqueue("second"),
            PreviewEnqueue::Coalesced { generation: 2 }
        );
        assert_eq!(
            coordinator.enqueue("newest"),
            PreviewEnqueue::Coalesced { generation: 3 }
        );

        let first = coordinator.complete(1);
        assert!(!first.accept_response);
        assert_eq!(first.next, Some((3, "newest")));
        assert_eq!(
            coordinator.complete(2),
            PreviewCompletion {
                accept_response: false,
                next: None
            }
        );
        assert!(coordinator.complete(3).accept_response);
    }

    #[test]
    fn cancelling_preview_work_drops_the_queued_cursor() {
        let mut coordinator = PreviewCoordinator::default();
        coordinator.enqueue("first");
        coordinator.enqueue("queued");
        coordinator.cancel();
        assert_eq!(
            coordinator.complete(1),
            PreviewCompletion {
                accept_response: false,
                next: None
            }
        );
    }
}
