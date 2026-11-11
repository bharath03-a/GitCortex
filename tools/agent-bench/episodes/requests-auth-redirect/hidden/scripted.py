"""Offline redirect harness for the requests-auth-redirect hidden tests."""

from __future__ import annotations

import os

import requests
from requests.adapters import BaseAdapter
from requests.models import Response
from requests.structures import CaseInsensitiveDict

TOKEN = "Bearer secret"


def checkout_is_under_test() -> bool:
    root = os.path.realpath(os.environ["EPISODE_CHECKOUT"])
    return os.path.realpath(requests.__file__).startswith(root)


class ScriptedAdapter(BaseAdapter):
    def __init__(self, redirects: dict[str, str]) -> None:
        super().__init__()
        self.redirects = redirects
        self.seen: list = []

    def send(self, request, **kwargs):
        self.seen.append(request)
        response = Response()
        response.url = request.url
        response.request = request
        response._content = b""
        response._content_consumed = True
        target = self.redirects.get(request.url)
        if target:
            response.status_code = 302
            response.headers = CaseInsensitiveDict({"Location": target})
        else:
            response.status_code = 200
        return response

    def close(self) -> None:
        return None


def follow(first_url: str, second_url: str):
    """Requests made when `first_url` redirects to `second_url`, sent with credentials."""
    adapter = ScriptedAdapter({first_url: second_url})
    session = requests.Session()
    session.trust_env = False
    session.mount("http://", adapter)
    session.mount("https://", adapter)
    session.get(first_url, headers={"Authorization": TOKEN})
    return adapter.seen
