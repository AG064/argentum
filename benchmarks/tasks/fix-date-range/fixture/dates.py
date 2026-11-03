from datetime import date, timedelta


def days_between(start: date, end: date) -> list[date]:
    """Every date from start to end, including both."""
    count = (end - start).days
    return [start + timedelta(days=offset) for offset in range(count)]
