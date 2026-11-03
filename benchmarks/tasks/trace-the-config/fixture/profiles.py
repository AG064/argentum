from defaults import TIMEOUT

PROFILES = {
    "development": {"timeout": TIMEOUT},
    "production": {"timeout": TIMEOUT * 2},
}
