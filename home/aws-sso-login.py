import os
import re
import subprocess
import sys

import pyotp
from playwright.sync_api import sync_playwright

sso_session = sys.argv[1]
username, password, totp_seed = (
    subprocess.run(
        ["secret-tool", "lookup", "service", "aws-sso-login", "key", key],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    for key in ("username", "password", "totp")
)
failure_screenshot = os.path.join(os.environ["STATE_DIRECTORY"], "failure.png")

login = subprocess.Popen(
    ["aws", "sso", "login", "--sso-session", sso_session, "--use-device-code", "--no-browser"],
    stdout=subprocess.PIPE,
    text=True,
)
device_url = next(match.group(0) for line in login.stdout if (match := re.search(r"https://\S+user_code=\S+", line)))

with sync_playwright() as playwright:
    page = playwright.chromium.launch().new_page()
    try:
        page.goto(device_url)
        page.get_by_label("Username").fill(username)
        page.get_by_role("button", name="Next").click()
        page.get_by_label("Password").fill(password)
        page.get_by_role("button", name="Sign in").click()
        page.get_by_label("MFA code").fill(pyotp.TOTP(totp_seed).now())
        page.get_by_role("button", name="Sign in").click()
        page.get_by_role("button", name="Confirm and continue").click()
        page.get_by_role("button", name="Allow access").click()
        page.get_by_text("Request approved").wait_for()
    except Exception:
        page.screenshot(path=failure_screenshot)
        login.kill()
        raise

sys.exit(login.wait())
