from app.security import verify_shared_secret


def test_shared_secret_allows_unconfigured_secret() -> None:
    assert verify_shared_secret(None, None) is True


def test_shared_secret_rejects_wrong_value() -> None:
    assert verify_shared_secret("wrong", "expected") is False


def test_shared_secret_accepts_match() -> None:
    assert verify_shared_secret("expected", "expected") is True
