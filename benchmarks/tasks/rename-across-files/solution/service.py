from store import load_user


def greeting(user_id: int) -> str:
    return f"Hello, {load_user(user_id)}"
