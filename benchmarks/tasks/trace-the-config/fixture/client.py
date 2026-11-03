from profiles import PROFILES

# Long uploads were failing, so production keeps a margin on top of its profile.
MARGIN = 15


def timeout(profile: str) -> int:
    value = PROFILES[profile]["timeout"]
    return value + MARGIN if profile == "production" else value
