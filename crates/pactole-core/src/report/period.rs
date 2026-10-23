use chrono::NaiveDate;

use super::ReportError;

/// An inclusive range of calendar days.
///
/// Both bounds belong to the period: a transaction dated on [`Period::start`]
/// or [`Period::end`] is part of the report. Use [`Period::new`] to build one,
/// or [`Period::single`] for a one-day period.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Period {
    start: NaiveDate,
    end: NaiveDate,
}

impl Period {
    /// Builds a period from two inclusive bounds.
    ///
    /// Returns [`ReportError::InvalidPeriod`] when `start` is after `end`.
    pub fn new(start: NaiveDate, end: NaiveDate) -> Result<Self, ReportError> {
        if start > end {
            return Err(ReportError::InvalidPeriod { start, end });
        }
        Ok(Self { start, end })
    }

    /// Builds a one-day period.
    pub fn single(date: NaiveDate) -> Self {
        Self {
            start: date,
            end: date,
        }
    }

    /// First day of the period, included.
    pub fn start(&self) -> NaiveDate {
        self.start
    }

    /// Last day of the period, included.
    pub fn end(&self) -> NaiveDate {
        self.end
    }

    /// Returns whether `date` belongs to the period, both bounds included.
    pub fn contains(&self, date: NaiveDate) -> bool {
        (self.start..=self.end).contains(&date)
    }
}

/// A calendar day used as a reporting cut-off.
///
/// A reference date always covers the whole day: a balance reported as of
/// `2026-01-31` includes every posting dated `2026-01-31`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReferenceDate(NaiveDate);

impl ReferenceDate {
    /// Builds a reference date.
    pub fn new(date: NaiveDate) -> Self {
        Self(date)
    }

    /// Returns the calendar day this reference date stands for.
    pub fn date(&self) -> NaiveDate {
        self.0
    }
}

impl From<NaiveDate> for ReferenceDate {
    fn from(date: NaiveDate) -> Self {
        Self::new(date)
    }
}

/// The date span a report covers.
///
/// Stock reports (a balance sheet) are evaluated at one point in time, while
/// flow reports (income, expenses) cover an inclusive period.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportScope {
    /// A balance as of a reference date, inclusive.
    AsOf(ReferenceDate),
    /// Movements over an inclusive period.
    Over(Period),
}

impl ReportScope {
    /// Builds a report scope evaluated at one reference date.
    pub fn as_of(date: NaiveDate) -> Self {
        Self::AsOf(ReferenceDate::new(date))
    }

    /// Builds a report scope covering an inclusive period.
    ///
    /// Returns [`ReportError::InvalidPeriod`] when `start` is after `end`.
    pub fn over(start: NaiveDate, end: NaiveDate) -> Result<Self, ReportError> {
        Ok(Self::Over(Period::new(start, end)?))
    }

    /// Returns whether `date` belongs to the scope.
    ///
    /// An [`ReportScope::AsOf`] scope contains every date up to and including
    /// its reference date.
    pub fn contains(&self, date: NaiveDate) -> bool {
        match self {
            Self::AsOf(reference) => date <= reference.date(),
            Self::Over(period) => period.contains(date),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    #[test]
    fn rejects_a_period_whose_start_is_after_its_end() {
        let error = Period::new(date(2024, 2, 1), date(2024, 1, 31)).unwrap_err();

        assert_eq!(
            error,
            ReportError::InvalidPeriod {
                start: date(2024, 2, 1),
                end: date(2024, 1, 31),
            }
        );
    }

    #[test]
    fn period_bounds_are_inclusive() {
        let period = Period::new(date(2024, 1, 10), date(2024, 1, 20)).unwrap();

        assert!(period.contains(date(2024, 1, 10)));
        assert!(period.contains(date(2024, 1, 20)));
        assert!(!period.contains(date(2024, 1, 9)));
        assert!(!period.contains(date(2024, 1, 21)));
    }

    #[test]
    fn a_single_day_period_contains_only_that_day() {
        let period = Period::single(date(2024, 1, 10));

        assert_eq!(period.start(), date(2024, 1, 10));
        assert_eq!(period.end(), date(2024, 1, 10));
        assert!(period.contains(date(2024, 1, 10)));
        assert!(!period.contains(date(2024, 1, 11)));
    }

    #[test]
    fn as_of_scope_contains_every_date_up_to_the_reference() {
        let scope = ReportScope::as_of(date(2024, 1, 31));

        assert!(scope.contains(date(1990, 1, 1)));
        assert!(scope.contains(date(2024, 1, 31)));
        assert!(!scope.contains(date(2024, 2, 1)));
    }

    #[test]
    fn over_scope_uses_an_inclusive_period() {
        let scope = ReportScope::over(date(2024, 1, 10), date(2024, 1, 20)).unwrap();

        assert!(!scope.contains(date(2024, 1, 9)));
        assert!(scope.contains(date(2024, 1, 10)));
        assert!(scope.contains(date(2024, 1, 20)));
        assert!(!scope.contains(date(2024, 1, 21)));
    }

    #[test]
    fn over_scope_rejects_an_inverted_period() {
        assert!(ReportScope::over(date(2024, 2, 1), date(2024, 1, 31)).is_err());
    }
}
