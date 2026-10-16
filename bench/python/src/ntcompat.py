"""One spelling for the pyntcore calls the bench makes, on 2026 and 2027."""

import ntcore

SNAKE = hasattr(ntcore.NetworkTableInstance, "start_client")

_RENAMED = {"start_client": "startClient4"}


def _camel(snake: str) -> str:
    head, *rest = snake.split("_")
    return head + "".join(word.title() for word in rest)


def call(obj, snake: str, *args, **kwargs):
    """Call the method `snake` names, spelled for this pyntcore.

    2026 spells most names as the camelCase of the 2027 name, and `start_client` as
    `startClient4`.
    """
    name = snake if SNAKE else _RENAMED.get(snake, _camel(snake))
    return getattr(obj, name)(*args, **kwargs)


def options(**snake_kwargs) -> ntcore.PubSubOptions:
    """PubSubOptions from snake_case keyword arguments."""
    if SNAKE:
        return ntcore.PubSubOptions(**snake_kwargs)
    return ntcore.PubSubOptions(**{_camel(key): value for key, value in snake_kwargs.items()})


def start_server(inst, port: int) -> None:
    """Serve NT4 on `port` with nothing persisted, and no NT3 on 2026."""
    if SNAKE:
        inst.start_server("", "", "", port)
    else:
        inst.startServer("", "", 0, port)


EVENT_TOPIC = ntcore.EventFlags.TOPIC if SNAKE else ntcore.EventFlags.kTopic
EVENT_VALUE_ALL = ntcore.EventFlags.VALUE_ALL if SNAKE else ntcore.EventFlags.kValueAll
