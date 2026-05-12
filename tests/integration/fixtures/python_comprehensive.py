"""Comprehensive Python fixture for GitCortex regression tests.

Covers: Protocol/Interface, @property, @staticmethod, @classmethod,
@dataclass, async methods, generator functions, nested classes,
module-level constants, type annotations, call detection, and imports.
"""
from __future__ import annotations

import os
import sys
from typing import Protocol, Optional, List
from dataclasses import dataclass

# ── Module-level constants ────────────────────────────────────────────────────

MAX_RETRIES = 3
DEFAULT_TIMEOUT = 30
API_VERSION = "v2"

# ── Protocols (mapped to Interface) ──────────────────────────────────────────

class Serializable(Protocol):
    def serialize(self) -> dict:
        ...

    def deserialize(self, data: dict) -> None:
        ...


class Repository(Protocol):
    def find_by_id(self, id: str) -> Optional[User]:
        ...

    def save(self, entity: User) -> User:
        ...


# ── Base class ────────────────────────────────────────────────────────────────

class BaseModel:
    def validate(self) -> bool:
        return True

    def to_dict(self) -> dict:
        return {}


# ── Dataclass with property / staticmethod / classmethod ─────────────────────

@dataclass
class User(BaseModel):
    name: str
    email: str
    age: Optional[int] = None

    @property
    def display_name(self) -> str:
        return f"{self.name} <{self.email}>"

    @staticmethod
    def from_dict(data: dict) -> User:
        return User(name=data["name"], email=data["email"])

    @classmethod
    def anonymous(cls) -> User:
        return cls(name="Anonymous", email="anon@example.com")

    def validate(self) -> bool:
        return bool(self.name and self.email)

    def _internal_check(self) -> bool:
        return True


# ── Async methods ─────────────────────────────────────────────────────────────

class AsyncService:
    async def fetch_user(self, user_id: str) -> Optional[User]:
        return None

    async def save_user(self, user: User) -> User:
        return user


# ── Generator functions ───────────────────────────────────────────────────────

def user_stream(users: List[User]):
    for user in users:
        yield user


async def async_user_stream(users: List[User]):
    for user in users:
        yield user


# ── Nested classes ────────────────────────────────────────────────────────────

class EventSystem:
    class Event:
        def __init__(self, name: str) -> None:
            self.name = name

    class Handler:
        def handle(self, event) -> None:
            pass

    def dispatch(self, event) -> None:
        pass


# ── Free functions with type annotations and cross-calls ─────────────────────

def create_user(name: str, email: str) -> User:
    return User(name=name, email=email)


def find_users(repository: Repository) -> List[User]:
    return []


def process_pipeline(users: List[User]) -> None:
    for user in user_stream(users):
        if user.validate():
            create_user(user.name, user.email)
