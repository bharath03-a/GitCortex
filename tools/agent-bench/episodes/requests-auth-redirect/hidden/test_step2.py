"""Step 2: credentials must never follow a redirect to another host, port, or lower scheme."""

from scripted import TOKEN, checkout_is_under_test, follow


def test_tests_run_against_the_episode_checkout():
    assert checkout_is_under_test()


def test_cross_host_redirect_strips_credentials():
    seen = follow("https://a.example.com/x", "https://b.example.net/y")
    assert seen[-1].url == "https://b.example.net/y"
    assert "Authorization" not in seen[-1].headers


def test_scheme_downgrade_on_same_host_strips_credentials():
    seen = follow("https://example.com/a", "http://example.com/b")
    assert "Authorization" not in seen[-1].headers


def test_other_port_on_same_host_strips_credentials():
    seen = follow("https://example.com/a", "https://example.com:8443/b")
    assert "Authorization" not in seen[-1].headers


def test_http_to_https_upgrade_keeps_credentials():
    seen = follow("http://example.com/a", "https://example.com/b")
    assert seen[-1].headers.get("Authorization") == TOKEN
