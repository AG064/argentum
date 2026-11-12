import store


def names(ids: list[int]) -> list[str]:
    return [store.load_user(user_id) for user_id in ids]
