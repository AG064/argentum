import store


def names(ids: list[int]) -> list[str]:
    return [store.fetch_user(user_id) for user_id in ids]
