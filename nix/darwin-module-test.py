"""Checks the launchd daemons that nix/darwin-module-test.nix configures."""

import json
import os
import plistlib
import shlex
import subprocess

daemons = [d for d in json.loads(os.environ["daemons"]) if d.startswith("jj-fetch")]
assert daemons == ["jj-fetch-alice", "jj-fetch-bob"], daemons
assert json.loads(os.environ["hasPackage"]), "jj-fetch isn't installed"


def check(user, home, args):
    with open(os.environ[user], "rb") as f:
        plist = plistlib.load(f)

    log = f"{home}/Library/Logs/jj-fetch.log"
    env = plist.pop("EnvironmentVariables")
    program = plist.pop("ProgramArguments")
    assert plist == {
        "Label": f"org.nixos.jj-fetch-{user}",
        "UserName": user,
        "RunAtLoad": True,
        "KeepAlive": True,
        "ProcessType": "Background",
        "StandardOutPath": log,
        "StandardErrorPath": log,
    }, plist

    assert env["HOME"] == home, env
    assert env["USER"] == user, env
    path = env["PATH"].split(":")
    assert path[0] == os.environ["git"], path
    assert "/usr/bin" in path, path

    # nix-darwin waits for the Nix store to be mounted, then runs the command.
    assert program[:2] == ["/bin/sh", "-c"], program
    prefix = "/bin/wait4path /nix/store && exec "
    assert program[2].startswith(prefix), program
    command = shlex.split(program[2].removeprefix(prefix))
    assert command[1:] == ["daemon", "--jj", os.environ["jj"], *args], command

    # The arguments are ones that the program accepts.
    options = command[: command.index("--")]
    subprocess.run([*options, "--help"], check=True, capture_output=True, timeout=10)


check("alice", "/Users/alice-home", ["--interval", "15m", "--max-depth", "3", "--", "/Users/alice-home/src"])
check("bob", "/Users/bob", ["--interval", "1h", "--max-depth", "5", "--", "/Users/bob/code", "/Users/bob/work stuff"])
