#!/usr/bin/env python3
"""Demo-feature acceptance with only fixed fictional browser data."""

from __future__ import annotations

import os
import socket
import subprocess
import tempfile
import time
from pathlib import Path

from acceptance import Driver, check_headers, wait_for_driver


def run_acceptance(driver: Driver, launch_url: str, downloads: Path) -> None:
    driver.request("POST", driver.path("/url"), {"url": launch_url})
    driver.wait_text("Browser-local studio ready.")
    driver.click_text("Files", "a")
    driver.wait_text("Public, non-secret password")
    if "oracle-demo" not in driver.body() or not driver.elements(".demo-controls"):
        raise RuntimeError("demo build did not visibly disclose its public password")

    driver.click_text("New scratch")
    driver.set_value(driver.control("form.save-scratch", "Public title"), "Unrelated Fictional Vault")
    driver.set_value(driver.control("form.save-scratch", "Password"), "unrelated-fixture-password")
    driver.click_text("Save encrypted vault")
    driver.wait_text("Unrelated Fictional Vault", timeout=120)
    driver.execute(
        "localStorage.setItem('oracle-studio.layout.v1', "
        "JSON.stringify({schema_version:1,left_collapsed:true,right_collapsed:false}));"
    )
    preference_before = driver.execute("return localStorage.getItem('oracle-studio.layout.v1')")

    driver.execute("window.confirm = () => false")
    driver.click_text("Load demo workspace")
    time.sleep(0.5)
    demo_cards = driver.elements(
        "//article[contains(@class,'vault-card')][.//h2[normalize-space()='Oracle Studio Demo']]",
        "xpath",
    )
    if demo_cards:
        raise RuntimeError("declining demo confirmation still installed a vault")

    driver.execute("window.confirm = () => true")
    driver.click_text("Load demo workspace")
    driver.wait(
        lambda: len(driver.elements(".vault-card")) == 2
        and "active"
        in driver.execute(
            "return arguments[0].innerText",
            [
                driver.element(
                    "//article[contains(@class,'vault-card')][.//h2[normalize-space()='Oracle Studio Demo']]",
                    "xpath",
                )
            ],
        ).casefold(),
        "active and unlocked demo vault",
        timeout=180,
    )
    driver.wait(
        lambda: driver.execute(
            "return Boolean(document.querySelector('#oracle-transit-biwheel')) "
            "&& !document.querySelector('.calculation-indicator').textContent.trim()"
        ),
        "initial demo synastry preview",
        timeout=180,
    )
    initial_problem = driver.execute(
        "return document.querySelector('.problem').textContent.trim()"
    )
    if initial_problem:
        raise RuntimeError(f"initial demo preview failed: {initial_problem}")
    initial_pair = driver.execute(
        "return [document.querySelector('.inner-meta').textContent, "
        "document.querySelector('.outer-meta').textContent]"
    )
    if "Avery North" not in initial_pair[0] or "Mira Vale" not in initial_pair[1]:
        raise RuntimeError(f"unexpected initial demo chart pair: {initial_pair}")
    demo_card = driver.element(
        "//article[contains(@class,'vault-card')][.//h2[normalize-space()='Oracle Studio Demo']]",
        "xpath",
    )
    if "active" not in driver.execute("return arguments[0].innerText", [demo_card]).casefold():
        raise RuntimeError("loaded demo vault is not active and unlocked")
    if "Unrelated Fictional Vault" not in driver.body():
        raise RuntimeError("loading the demo removed an unrelated vault")
    print("PASS demo load: confirmation, stable import, unlock, and unrelated-vault preservation")

    driver.click_text("Settings", "a")
    driver.wait_text("Avery North")
    driver.execute("document.querySelector('details.advanced').open = true")
    counts = driver.execute(
        """
        const output = {};
        for (const panel of document.querySelectorAll('.settings-panel')) {
          const heading = panel.querySelector('h2')?.textContent.trim()
            || panel.querySelector('summary strong')?.textContent.trim();
          if (heading) output[heading] = panel.querySelectorAll('.entity-list li').length;
        }
        return output;
        """
    )
    expected = {"People": 2, "Locations / GeoNames": 2, "Charts": 4, "Comparison records": 3}
    for heading, count in expected.items():
        if counts.get(heading) != count:
            raise RuntimeError(f"demo record count mismatch for {heading}: {counts}")
    for label in (
        "Avery North",
        "Mira Vale",
        "Juniper Harbor",
        "Cedar Observatory",
        "Harbor Transit",
        "Cedar Equinox Event",
        "Avery North + Mira Vale synastry",
    ):
        if label not in driver.body():
            raise RuntimeError(f"demo is missing fixed fictional record {label!r}")
    print("PASS demo records: two people, two locations, four charts, and three comparisons")

    driver.click_text("Workbench", "a")
    transit_card = driver.element(
        "//article[.//strong[normalize-space()='Harbor Transit']]", "xpath"
    )
    driver.click(driver.child(transit_card, ".//button[normalize-space()='Use as Outer']", "xpath"))
    driver.wait(
        lambda: driver.execute(
            "return document.querySelector('.outer-meta').textContent.includes('Harbor Transit') "
            "&& Boolean(document.querySelector('#oracle-transit-biwheel'))"
        ),
        "demo Moshier workbench wheel",
        timeout=180,
    )
    print("PASS demo chart: fixed Harbor Transit renders in the Moshier workbench")

    driver.click_text("Files", "a")
    demo_card = driver.element(
        "//article[contains(@class,'vault-card')][.//h2[normalize-space()='Oracle Studio Demo']]",
        "xpath",
    )
    driver.click(driver.child(demo_card, ".//button[normalize-space()='Export']", "xpath"))
    driver.wait(
        lambda: any(downloads.glob("oracle-studio-demo*.oracle-vault")),
        "demo vault export",
    )
    exported = next(downloads.glob("oracle-studio-demo*.oracle-vault"))
    if exported.stat().st_size < 1024:
        raise RuntimeError("exported demo vault is unexpectedly small")
    driver.click(driver.child(demo_card, ".//button[normalize-space()='Lock']", "xpath"))
    driver.wait(
        lambda: "locked" in driver.execute(
            "return arguments[0].innerText",
            [
                driver.element(
                    "//article[contains(@class,'vault-card')][.//h2[normalize-space()='Oracle Studio Demo']]",
                    "xpath",
                )
            ],
        ).casefold(),
        "locked demo vault",
    )
    demo_card = driver.element(
        "//article[contains(@class,'vault-card')][.//h2[normalize-space()='Oracle Studio Demo']]",
        "xpath",
    )
    driver.set_value(driver.child(demo_card, ".//input[@type='password']", "xpath"), "oracle-demo")
    driver.click(driver.child(demo_card, ".//button[normalize-space()='Unlock']", "xpath"))
    driver.wait(
        lambda: "active" in driver.execute(
            "return arguments[0].innerText",
            [
                driver.element(
                    "//article[contains(@class,'vault-card')][.//h2[normalize-space()='Oracle Studio Demo']]",
                    "xpath",
                )
            ],
        ).casefold(),
        "public-password demo unlock",
        timeout=120,
    )

    driver.execute("window.__demoConfirmCount = 0; window.confirm = () => { window.__demoConfirmCount++; return true; }")
    driver.click_text("Reset demo workspace")
    driver.wait(
        lambda: driver.execute("return window.__demoConfirmCount === 1"),
        "demo reset confirmation",
    )
    driver.wait(
        lambda: driver.execute(
            "return ![...document.querySelectorAll('.demo-controls button')].some(button => button.disabled)"
        ),
        "demo reset completion",
        timeout=180,
    )
    if len(driver.elements(".vault-card")) != 2 or "Unrelated Fictional Vault" not in driver.body():
        raise RuntimeError("reset changed an unrelated browser vault")
    if driver.execute("return localStorage.getItem('oracle-studio.layout.v1')") != preference_before:
        raise RuntimeError("demo load or reset changed a global layout preference")
    print("PASS demo reset: export, lock/unlock, targeted replacement, and preferences are safe")

    browser_log = driver.request("POST", driver.path("/log"), {"type": "browser"})
    csp_blocks = [
        entry
        for entry in browser_log
        if "Content Security Policy" in entry.get("message", "")
        or "violates the following Content Security Policy" in entry.get("message", "")
    ]
    if csp_blocks:
        raise RuntimeError(f"demo runtime content was blocked by CSP: {csp_blocks}")
    print("PASS demo browser: no CSP violations")


def main() -> int:
    launch_url = os.environ.get("ORACLE_STUDIO_URL", "http://127.0.0.1:8080/")
    if launch_url != "http://127.0.0.1:8080/":
        raise RuntimeError("demo acceptance requires the stable loopback origin")
    check_headers(launch_url)
    downloads = Path("/tmp/oracle-demo-downloads")
    downloads.mkdir(mode=0o700)
    with socket.socket() as port_socket:
        port_socket.bind(("127.0.0.1", 0))
        port = port_socket.getsockname()[1]
    driver_url = f"http://127.0.0.1:{port}"
    process = subprocess.Popen(
        [os.environ["CHROMEDRIVER_BIN"], f"--port={port}", "--allowed-ips=127.0.0.1", "--log-level=WARNING"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    driver = Driver(driver_url)
    try:
        wait_for_driver(driver_url)
        with tempfile.TemporaryDirectory(prefix="oracle-demo-chrome-") as profile:
            session = driver.request(
                "POST",
                "/session",
                {
                    "capabilities": {
                        "alwaysMatch": {
                            "browserName": "chrome",
                            "goog:loggingPrefs": {"browser": "ALL"},
                            "goog:chromeOptions": {
                                "binary": os.environ["CHROME_BIN"],
                                "prefs": {
                                    "download.default_directory": str(downloads),
                                    "download.prompt_for_download": False,
                                },
                                "args": [
                                    "--headless=new", "--no-sandbox", "--disable-gpu",
                                    "--disable-dev-shm-usage", "--disable-background-networking",
                                    "--disable-component-update", "--disable-sync", "--metrics-recording-only",
                                    "--no-first-run", "--no-default-browser-check", "--password-store=basic",
                                    "--use-mock-keychain", "--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE 127.0.0.1",
                                    "--window-size=1440,1200", f"--user-data-dir={profile}",
                                ],
                            },
                        }
                    }
                },
            )
            driver.session = session.get("sessionId")
            if not driver.session:
                raise RuntimeError("ChromeDriver did not return a session ID")
            run_acceptance(driver, launch_url, downloads)
    finally:
        if driver.session:
            try:
                driver.request("DELETE", driver.path())
            except Exception:
                pass
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
