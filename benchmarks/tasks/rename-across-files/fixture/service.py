from store import fetch_user


def greeting(user_id: int) -> str:
    return f"Hello, {fetch_user(user_id)}"
