"""The test report: what passed, what failed and why, what the regression
suite found, and the warnings, from the logs of a run, as Markdown.

    python3 -I ci/report.py test.log regression.txt build-warnings.txt > report.md

Written for whoever has to act on it, a person or an assistant: every failed
test with its output, every template whose numbers moved with both values,
and the command to run each again.
"""
import re, sys, os

test_log, regress_txt, warn_txt = sys.argv[1:4]
log = open(test_log, errors="replace").read()

# Tests, one line each: "test name ... ok|FAILED|ignored"
results = re.findall(r"^test (\S+) \.\.\. (ok|FAILED|ignored)(?: \(.*?\))?$", log, re.M)
passed = [n for n, r in results if r == "ok"]
failed = [n for n, r in results if r == "FAILED"]
ignored = [n for n, r in results if r == "ignored"]

# The output of each failed test, from the "failures:" section.
outputs = {}
for m in re.finditer(r"^---- (\S+) stdout ----\n(.*?)(?=^---- \S+ stdout ----\n|^failures:\n|^test result:)", log, re.M | re.S):
    outputs[m.group(1)] = m.group(2).strip()

build_failed = "error: could not compile" in log or "error[E" in log
summary_line = re.search(r"^test result: .*$", log, re.M)

def section(title):
    print(f"\n## {title}\n")

commit = os.environ.get("GITHUB_SHA", "")[:12]
branch = os.environ.get("GITHUB_REF_NAME", "")
print(f"# XMS test report{' for ' + commit if commit else ''}{' on ' + branch if branch else ''}\n")
if build_failed:
    print("**The build failed.** The tests did not run.\n")
    errs = re.findall(r"^error(\[E\d+\])?: .*?(?=\n\n)", log, re.M | re.S)
    for e in re.finditer(r"^(error(\[E\d+\])?: [^\n]*\n(?:\s+-->[^\n]*\n)?)", log, re.M):
        print("- `" + e.group(1).strip().replace("\n", " ") + "`")
else:
    print(f"**{len(passed)} passed, {len(failed)} failed, {len(ignored)} ignored.** "
          f"{'All tests pass.' if not failed else 'Fix the failures below.'}\n")

if failed:
    section("Failed tests")
    for n in failed:
        print(f"### `{n}`\n")
        print(f"Run it again with `cargo test --release {n.split('::')[-1]} -- --nocapture`.\n")
        out = outputs.get(n, "(no output captured)")
        # The panic message first, then the rest, trimmed.
        panic = re.search(r"thread '.*?' panicked at [^\n]*\n([^\n]*)", out)
        if panic: print(f"**{panic.group(0).strip().splitlines()[0]}**\n\n{panic.group(1).strip()}\n")
        # Lines that say nothing about the failure are left out: the
        # regression suite's "ok" lines, the backtrace.
        kept = [l for l in out.splitlines() if not re.match(r"^(ok\s|stack backtrace:|\s+\d+: |note: Some details)", l)]
        out = "\n".join(kept).strip()
        body = out if len(out) < 6000 else out[:6000] + "\n... (cut; the whole log is in the artifact)"
        print("```\n" + body + "\n```\n")

if os.path.isfile(regress_txt):
    lines = open(regress_txt, errors="replace").read().splitlines()
    skipped = [l for l in lines if l.startswith("SKIPPED")]
    changed = [l for l in lines if not l.startswith("ok") and not l.startswith("SKIPPED")]
    section("Regression suite: every template at its view flag")
    if not lines:
        print("No report: the regression test did not run (see the failures above).\n")
    elif changed:
        print(f"**{len(changed)} template(s) differ from the baseline** (`tests/regression/templates.json`). "
              "Each line names the template and the fields that moved, as `was X, now Y`. "
              "A change made on purpose is taken into the baseline with `XMS_REGRESS_UPDATE=1 cargo test --release regression` and committed; "
              "anything else is a regression to find in the commits since the baseline was last written.\n")
        for l in changed: print(f"- `{l}`")
        print()
    else:
        print(f"All {len(lines) - len(skipped)} templates give the numbers of the baseline.\n")
    if skipped:
        print(f"{len(skipped)} template(s) skipped, their example files not downloaded (the download step did not run or failed):\n")
        for l in skipped: print(f"- `{l}`")
        print()
    timings = []
    for l in lines:
        m = re.match(r"ok\s+(.*?) \((\d+) ms\)$", l)
        if m: timings.append((int(m.group(2)), m.group(1)))
    timings.sort(reverse=True)
    if timings:
        print("Slowest cooks: " + ", ".join(f"{n} {t} ms" for t, n in timings[:5]) + ".\n")

if os.path.isfile(warn_txt):
    warns = [l for l in open(warn_txt, errors="replace").read().splitlines() if l.startswith("warning:")]
    section("Build warnings")
    kinds = {}
    for w in warns:
        k = re.sub(r"`[^`]*`", "`…`", w[len("warning: "):])
        kinds[k] = kinds.get(k, 0) + 1
    print(f"{len(warns)} warning(s) in the build of the program.\n")
    for k, n in sorted(kinds.items(), key=lambda kv: -kv[1])[:12]: print(f"- {n} × {k}")
    print()

if summary_line and not build_failed:
    section("cargo test")
    print("```\n" + summary_line.group(0) + "\n```")
