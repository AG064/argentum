USERS = {1: "ada", 2: "grace"}


def fetch_user(user_id: int) -> str:
    return USERS[user_id]
