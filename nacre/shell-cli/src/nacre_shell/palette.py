"""Compatibility API for the Rust palette policies."""

from ._backend import call

_constants = call("constants")
DEFAULT_SEED = _constants["DEFAULT_SEED"]
SEMANTIC_SEEDS = _constants["SEMANTIC_SEEDS"]
VARIANTS = tuple(_constants["VARIANTS"])
PERSONALITIES = tuple(_constants["PERSONALITIES"])
REQUIRED_ROLES = frozenset(_constants["REQUIRED_ROLES"])


def supporting_sources(seed, candidates, harmony=False):
    return call("supporting_sources", seed=seed, candidates=list(candidates), harmony=bool(harmony))


def background_policy(personality, background_from_wallpaper=None):
    return tuple(
        call("background_policy", personality=personality, background_from_wallpaper=background_from_wallpaper)
    )


def generate(
    seed,
    mode="dark",
    variant="tonalspot",
    flavour="default",
    companions=(),
    harmony=False,
    body=None,
    personality=None,
    overrides=None,
    brightness=0.0,
    saturation=1.0,
    hour=12,
    background_from_wallpaper=None,
    time_tint=False,
):
    options = dict(locals())
    options.pop("seed")
    options["companions"] = list(companions)
    options["harmony"] = bool(harmony)
    options["time_tint"] = bool(time_tint)
    return call("generate", seed=seed, options=options)


def validate(colors):
    return call("validate", colors=colors)
