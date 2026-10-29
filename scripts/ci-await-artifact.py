#!/usr/bin/env python3
"""Waits for an artifact of this workflow run and unpacks it, so a job can use what a
sibling job builds without waiting for that job to start it.
  scripts/ci-await-artifact.py NAME JOB DIR READY
Unpacks artifact NAME into DIR, then writes `ok` to READY; writes `failed` once job JOB
ended without it. Needs GITHUB_TOKEN and the `actions: read` permission."""
import io, json, os, sys, time, urllib.error, urllib.request, zipfile

name, job, out, ready = sys.argv[1:5]
api = f"{os.environ.get('GITHUB_API_URL', 'https://api.github.com')}/repos/{os.environ['GITHUB_REPOSITORY']}/actions"
run = f"{api}/runs/{os.environ['GITHUB_RUN_ID']}"
auth = {"Authorization": f"Bearer {os.environ['GITHUB_TOKEN']}", "Accept": "application/vnd.github+json"}


class Keep(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args):  # the signed blob URL refuses the token
        return None


def get(url, opener=urllib.request.build_opener()):
    return opener.open(urllib.request.Request(url, headers=auth), timeout=60)


def done(state):
    with open(ready, "w") as f:
        f.write(state)
    print(f"{name}: {state}", file=sys.stderr)
    sys.exit(state != "ok")


def wait():
    start = time.monotonic()
    while True:
        try:
            found = json.load(get(f"{run}/artifacts?name={name}"))["artifacts"]
            if found:
                try:
                    get(found[0]["archive_download_url"], urllib.request.build_opener(Keep))
                    raise RuntimeError("no redirect to the archive")
                except urllib.error.HTTPError as e:
                    if e.code != 302:
                        raise
                    data = urllib.request.urlopen(e.headers["Location"], timeout=600).read()
                zipfile.ZipFile(io.BytesIO(data)).extractall(out)
                done("ok")
            jobs = json.load(get(f"{run}/jobs?per_page=100"))["jobs"]
            if any(j["name"] == job and j["status"] == "completed" and j["conclusion"] != "success" for j in jobs):
                done("failed")
        except (urllib.error.URLError, OSError, KeyError, ValueError) as e:
            print(f"{name}: {e}, retrying", file=sys.stderr)
        took = int(time.monotonic() - start)
        print(f"{name}: waiting for job {job} ({took // 60}m{took % 60:02d}s)", file=sys.stderr)
        time.sleep(15)


try:
    wait()
except Exception as e:  # whoever waits on READY must not wait forever
    print(f"{name}: {e}", file=sys.stderr)
    done("failed")
