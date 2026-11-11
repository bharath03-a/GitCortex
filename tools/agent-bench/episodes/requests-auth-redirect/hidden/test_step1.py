"""Step 1: explicit default ports must not cost a same-host redirect its credentials."""

from scripted import TOKEN, checkout_is_under_test, follow


def test_tests_run_against_the_episode_checkout():
    assert checkout_is_under_test()


def test_explicit_https_port_on_source_keeps_credentials():
    seen = follow("https://example.com:443/a", "https://example.com/b")
    assert seen[-1].url == "https://example.com/b"
    assert seen[-1].headers.get("Authorization") == TOKEN


def test_explicit_https_port_on_target_keeps_credentials():
    seen = follow("https://example.com/a", "https://example.com:443/b")
    assert seen[-1].url == "https://example.com:443/b"
    assert seen[-1].headers.get("Authorization") == TOKEN


def test_explicit_http_port_keeps_credentials():
    seen = follow("http://example.com:80/a", "http://example.com/b")
    assert seen[-1].headers.get("Authorization") == TOKEN
