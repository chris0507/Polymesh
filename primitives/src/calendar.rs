use chrono::{Datelike, NaiveDate, NaiveDateTime};
use codec::{Decode, Encode};
#[cfg(feature = "std")]
use sp_runtime::{Deserialize, Serialize};
use sp_std::convert::TryFrom;

/// Calendar units for timing recurring operations.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Encode, Decode, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalendarUnit {
    /// A unit of one second.
    Second,
    /// A unit of one minute.
    Minute,
    /// A unit of one hour.
    Hour,
    /// A unit of one day.
    Day,
    /// A unit of one week.
    Week,
    /// A unit of one month.
    Month,
    /// A unit of one year.
    Year,
}

impl Default for CalendarUnit {
    fn default() -> Self {
        Self::Second
    }
}

/// Calendar units that have variable length.
pub enum VariableCalendarUnit {
    /// A unit of one month.
    Month,
    /// A unit of one year.
    Year,
}

/// Either a duration in seconds or a variable length calendar unit.
pub enum FixedOrVariableCalendarUnit {
    /// A fixed duration in seconds Unix time, not counting leap seconds.
    Fixed(u64),
    /// A variable length calendar unit.
    Variable(VariableCalendarUnit),
}

/// A simple period which is a multiple of a `CalendarUnit`.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Encode, Decode, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CalendarPeriod {
    /// The base calendar unit.
    pub unit: CalendarUnit,
    /// The number of base units in the period.
    pub multiplier: u64,
}

impl CalendarPeriod {
    /// For fixed length calendar periods (in Unix time, without leap seconds), computes the number
    /// of seconds they contain. For variable length periods, returns a
    /// `VariableLengthCalendarPeriod`.
    pub fn as_fixed_or_variable(&self) -> FixedOrVariableCalendarUnit {
        match self.unit {
            CalendarUnit::Second => FixedOrVariableCalendarUnit::Fixed(self.multiplier),
            CalendarUnit::Minute => FixedOrVariableCalendarUnit::Fixed(self.multiplier * 60),
            CalendarUnit::Hour => FixedOrVariableCalendarUnit::Fixed(self.multiplier * 60 * 60),
            CalendarUnit::Day => FixedOrVariableCalendarUnit::Fixed(self.multiplier * 60 * 60 * 24),
            CalendarUnit::Week => {
                FixedOrVariableCalendarUnit::Fixed(self.multiplier * 60 * 60 * 24 * 7)
            }
            CalendarUnit::Month => {
                FixedOrVariableCalendarUnit::Variable(VariableCalendarUnit::Month)
            }
            CalendarUnit::Year => FixedOrVariableCalendarUnit::Variable(VariableCalendarUnit::Year),
        }
    }
}

/// The schedule of an asset checkpoint containing the start time `start` and the optional period
/// `period` - defined with a non-0 multiplier - in case the checkpoint is to recur after `start` at
/// regular intervals.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Encode, Decode, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CheckpointSchedule {
    /// Unix time in seconds.
    pub start: u64,
    /// The period at which the checkpoint is set to recur after `start`.
    pub period: CalendarPeriod,
}

impl CheckpointSchedule {
    /// Computes the next checkpoint for the schedule given the current timestamp in seconds Unix
    /// time.
    ///
    /// If the schedule period unit is fixed - from seconds to weeks - the function adds the length
    /// of that period in seconds to `now_as_secs_utc`.
    ///
    /// If that unit is not fixed then it is variable - months or years - meaning that, for example,
    /// the last days of every month do not have the same number. In that case the next timestamp
    /// computation takes into account the day of the month at the start of the schedule. Every
    /// checkpoint gets a day of the month that is at most that of the start of the schedule.
    ///
    /// # Example
    ///
    /// Assume we have the checkpoint schedule that
    ///
    /// * starts on 00:01:00 the 31st of January, 2021 (Unix time) and
    /// * has a period of one month.
    ///
    /// The checkpoint timestamps are going to be
    ///
    /// * 2021-01-31T00:01:00
    /// * 2021-02-28T00:01:00
    /// * 2021-03-31T00:01:00
    /// * 2020-04-30T00:01:00
    ///
    /// and so on.
    pub fn next_checkpoint(&self, now_as_secs_utc: u64) -> Option<u64> {
        if self.start > now_as_secs_utc {
            // The start time is in the future.
            return Some(self.start);
        }
        // The start time is in the past.
        let multiplier = self.period.multiplier;
        if multiplier == 0 {
            // The period is empty while the start time has already passed.
            return None;
        }
        match self.period.as_fixed_or_variable() {
            FixedOrVariableCalendarUnit::Fixed(period_as_secs) => {
                // The period is of fixed length in seconds Unix time.
                let secs_since_start = now_as_secs_utc - self.start;
                let elapsed_periods: u64 = secs_since_start / period_as_secs;
                Some(self.start + period_as_secs * (elapsed_periods + 1))
            }
            FixedOrVariableCalendarUnit::Variable(variable_unit) => {
                // The period is of variable length.
                let date_time_start =
                    NaiveDateTime::from_timestamp(i64::try_from(self.start).ok()?, 0);
                let date_start = date_time_start.date();
                let year_start = date_start.year();
                let month_start = date_start.month();
                let day_start = date_start.day();
                let date_time_now =
                    NaiveDateTime::from_timestamp(i64::try_from(now_as_secs_utc).ok()?, 0);
                let date_now = date_time_now.date();
                let date_next = match variable_unit {
                    VariableCalendarUnit::Month => {
                        // Convert the multiplier to match the type of month.
                        let multiplier = u32::try_from(multiplier).ok()?;
                        let year_diff = u32::try_from(date_now.year() - year_start).ok()?;
                        // Convert months to base 12.
                        let month_start_0_indexed = month_start - 1;
                        let elapsed_months =
                            year_diff * 12 + date_now.month0() - month_start_0_indexed;
                        let elapsed_periods = elapsed_months / multiplier;
                        let next_period_months = multiplier * (elapsed_periods + 1);
                        // The month of the next period counting from the beginning of `year_start`.
                        let denormalized_next_period_month =
                            month_start_0_indexed + next_period_months;
                        let next_period_year =
                            year_start + i32::try_from(denormalized_next_period_month / 12).ok()?;
                        // Month in base 12.
                        let next_period_month = denormalized_next_period_month % 12;
                        let max_month_days = match next_period_month {
                            // Handle 30-day months.
                            3 | 5 | 8 | 10 => 30,
                            // Handle February.
                            1 if is_leap_year(next_period_year) => 29,
                            1 => 28,
                            // No month has more than 31 days.
                            _ => 31,
                        };
                        let next_period_day = u32::min(day_start, max_month_days);
                        NaiveDate::from_ymd(
                            i32::try_from(next_period_year).ok()?,
                            // Convert months back to calendar numbering.
                            next_period_month + 1,
                            next_period_day,
                        )
                    }
                    VariableCalendarUnit::Year => {
                        // Convert the multiplier to match the type of year.
                        let multiplier = i32::try_from(multiplier).ok()?;
                        let elapsed_periods: i32 = (date_now.year() - year_start) / multiplier;
                        let next_period_year = year_start + multiplier * (elapsed_periods + 1);
                        let next_period_day = if month_start == 2 && !is_leap_year(next_period_year)
                        {
                            // Handle February in non-leap years.
                            u32::min(day_start, 28)
                        } else {
                            day_start
                        };
                        NaiveDate::from_ymd(
                            next_period_year,
                            // Months are already in calendar numbering.
                            month_start,
                            next_period_day,
                        )
                    }
                };
                u64::try_from(date_next.and_time(date_time_start.time()).timestamp()).ok()
            }
        }
    }
}

/// Copy of a function from the `time` crate.
///
/// Returns if the provided year is a leap year in the proleptic Gregorian calendar. Uses
/// [astronomical year numbering](https://en.wikipedia.org/wiki/Astronomical_year_numbering).
///
/// ```rust
/// use polymesh_primitives::calendar::is_leap_year;
/// assert!(!is_leap_year(1900));
/// assert!(is_leap_year(2000));
/// assert!(is_leap_year(2004));
/// assert!(!is_leap_year(2005));
/// assert!(!is_leap_year(2100));
/// ```
#[inline(always)]
pub fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0) & ((year % 100 != 0) | (year % 400 == 0))
}

#[cfg(test)]
mod tests {
    use super::{CalendarPeriod, CalendarUnit, CheckpointSchedule};
    use chrono::{NaiveDate, NaiveDateTime, NaiveTime};

    fn format_date_time(timestamp: i64) -> String {
        format!("{}", NaiveDateTime::from_timestamp(timestamp, 0))
    }

    #[test]
    fn next_checkpoint_seconds_test() {
        let period_day_seconds = CalendarPeriod {
            unit: CalendarUnit::Second,
            multiplier: 60 * 60 * 24,
        };
        let schedule_day_seconds = CheckpointSchedule {
            start: 60 * 60, // 1:00:00
            period: period_day_seconds,
        };
        let checkpoint1 = schedule_day_seconds.next_checkpoint(
            NaiveDate::from_ymd(1970, 01, 01)
                .and_time(NaiveTime::from_hms(1, 0, 0))
                .timestamp() as u64,
        );
        assert_eq!(
            format_date_time(checkpoint1.unwrap() as i64),
            format_date_time(
                NaiveDate::from_ymd(1970, 01, 02)
                    .and_time(NaiveTime::from_hms(1, 0, 0))
                    .timestamp()
            )
        );
        let checkpoint2 = schedule_day_seconds.next_checkpoint(
            NaiveDate::from_ymd(2020, 12, 12)
                .and_time(NaiveTime::from_hms(1, 2, 3))
                .timestamp() as u64,
        );
        assert_eq!(
            format_date_time(checkpoint2.unwrap() as i64),
            format_date_time(
                NaiveDate::from_ymd(2020, 12, 13)
                    .and_time(NaiveTime::from_hms(1, 0, 0))
                    .timestamp()
            )
        );
    }

    #[test]
    fn next_checkpoint_months_test() {
        let period_5_months = CalendarPeriod {
            unit: CalendarUnit::Month,
            multiplier: 5,
        };
        let schedule_5_months = CheckpointSchedule {
            start: 0,
            period: period_5_months,
        };
        let checkpoint = schedule_5_months.next_checkpoint(
            NaiveDate::from_ymd(1970, 4, 1)
                .and_time(NaiveTime::from_hms(1, 2, 3))
                .timestamp() as u64,
        );
        assert_eq!(
            format_date_time(checkpoint.unwrap() as i64),
            format_date_time(
                NaiveDate::from_ymd(1970, 6, 1)
                    .and_time(NaiveTime::from_hms(0, 0, 0))
                    .timestamp()
            )
        );
    }

    #[test]
    fn next_checkpoint_end_of_month_test() {
        let period_1_month = CalendarPeriod {
            unit: CalendarUnit::Month,
            multiplier: 1,
        };
        let schedule_end_of_month = CheckpointSchedule {
            start: NaiveDate::from_ymd(2024, 1, 31)
                .and_time(NaiveTime::from_hms(1, 2, 3))
                .timestamp() as u64,
            period: period_1_month,
        };
        let checkpoint_leap_feb = schedule_end_of_month.next_checkpoint(
            NaiveDate::from_ymd(2024, 1, 31)
                .and_time(NaiveTime::from_hms(1, 2, 30))
                .timestamp() as u64,
        );
        assert_eq!(
            format_date_time(checkpoint_leap_feb.unwrap() as i64),
            format_date_time(
                NaiveDate::from_ymd(2024, 2, 29)
                    .and_time(NaiveTime::from_hms(1, 2, 3))
                    .timestamp()
            )
        );
        let checkpoint_nonleap_feb = schedule_end_of_month.next_checkpoint(
            NaiveDate::from_ymd(2025, 1, 31)
                .and_time(NaiveTime::from_hms(1, 2, 30))
                .timestamp() as u64,
        );
        assert_eq!(
            format_date_time(checkpoint_nonleap_feb.unwrap() as i64),
            format_date_time(
                NaiveDate::from_ymd(2025, 2, 28)
                    .and_time(NaiveTime::from_hms(1, 2, 3))
                    .timestamp()
            )
        );
        let checkpoint_apr = schedule_end_of_month.next_checkpoint(
            NaiveDate::from_ymd(2025, 3, 31)
                .and_time(NaiveTime::from_hms(1, 2, 30))
                .timestamp() as u64,
        );
        assert_eq!(
            format_date_time(checkpoint_apr.unwrap() as i64),
            format_date_time(
                NaiveDate::from_ymd(2025, 4, 30)
                    .and_time(NaiveTime::from_hms(1, 2, 3))
                    .timestamp()
            )
        );
    }
}
