#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Date(u64);

impl Date {
    const TICKS_IN_HOUR: u64 = 3;
    const HOURS_IN_DAY: u64 = 24;
    const DAYS_IN_MONTH: u64 = 30;
    const MONTHS_IN_YEAR: u64 = 12;

    const TICKS_IN_DAY: u64 = Self::TICKS_IN_HOUR * Self::HOURS_IN_DAY;
    const TICKS_IN_MONTH: u64 = Self::TICKS_IN_DAY * Self::DAYS_IN_MONTH;
    const TICKS_IN_YEAR: u64 = Self::TICKS_IN_MONTH * Self::MONTHS_IN_YEAR;

    pub fn with_calendar(day: u64, month: u64, year: u64) -> Self {
        assert!(day > 0);
        assert!(month > 0);
        Self(
            (day - 1) * Self::TICKS_IN_DAY
                + (month - 1) * Self::TICKS_IN_MONTH
                + year * Self::TICKS_IN_YEAR,
        )
    }

    pub fn advance(&mut self) {
        self.0 += 1;
    }

    pub fn calendar_day(&self) -> u64 {
        (self.0 / Self::TICKS_IN_DAY) % Self::DAYS_IN_MONTH + 1
    }

    pub fn calendar_month(&self) -> u64 {
        (self.0 / Self::TICKS_IN_MONTH) % Self::MONTHS_IN_YEAR + 1
    }

    pub fn calendar_year(&self) -> u64 {
        self.0 / Self::TICKS_IN_YEAR + 1
    }
}
