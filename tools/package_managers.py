"""Distro package transactions; configuration setup never builds source packages."""

import re
import subprocess


class Arch:
    @staticmethod
    def missing(packages):
        result = subprocess.run(
            ["pacman", "-T", *packages], capture_output=True, text=True, check=False
        )
        if result.returncode not in (0, 127):
            raise RuntimeError(
                "Could not check installed package dependencies: "
                + result.stderr.strip()
            )
        return result.stdout.splitlines()

    @staticmethod
    def install(packages):
        if any(
            not re.fullmatch(r"[a-zA-Z0-9@_+.:<>=-]+", name) or name.startswith("-")
            for name in packages
        ):
            raise ValueError("Invalid package dependency")
        # A full upgrade keeps Arch/CachyOS libraries coherent. Pacman's normal
        # transaction preview/password prompt remains the owner of installation.
        try:
            from package_transaction import transaction

            status = transaction(packages)
            if status:
                raise subprocess.CalledProcessError(status, "pacman")
        except subprocess.CalledProcessError as error:
            raise RuntimeError(
                "Package installation did not complete; no configuration was applied. "
                "Resolve the pacman error and run nacre setup again."
            ) from error


def manager(distribution):
    adapters = {"arch": Arch}
    if distribution not in adapters:
        raise RuntimeError("No package-manager adapter for " + distribution)
    return adapters[distribution]
