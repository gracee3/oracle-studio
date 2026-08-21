#!/usr/bin/env python3
"""Fictional-data Chrome acceptance for the static browser-local product."""

from __future__ import annotations

import json
import os
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
import zipfile
from pathlib import Path
from typing import Any, Callable

ELEMENT = "element-6066-11e4-a52e-4f735466cecf"


class Driver:
    def __init__(self, url: str) -> None:
        self.url = url
        self.session: str | None = None

    def request(self, method: str, path: str, payload: Any = None, timeout: int = 45) -> Any:
        data = None if payload is None else json.dumps(payload).encode()
        request = urllib.request.Request(
            self.url + path,
            data=data,
            method=method,
            headers={"Content-Type": "application/json"},
        )
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                result = json.load(response)
        except urllib.error.HTTPError as error:
            body = error.read().decode(errors="replace")
            raise RuntimeError(f"WebDriver {method} {path}: HTTP {error.code}: {body[:800]}") from error
        value = result.get("value")
        if isinstance(value, dict) and value.get("error"):
            raise RuntimeError(f"WebDriver {value['error']}: {value.get('message', '')}")
        return value

    def path(self, suffix: str = "") -> str:
        if self.session is None:
            raise RuntimeError("WebDriver session is not open")
        return f"/session/{self.session}{suffix}"

    def element(self, selector: str, using: str = "css selector") -> dict[str, str]:
        return self.request("POST", self.path("/element"), {"using": using, "value": selector})

    def elements(self, selector: str, using: str = "css selector") -> list[dict[str, str]]:
        return self.request("POST", self.path("/elements"), {"using": using, "value": selector})

    def child(self, parent: dict[str, str], selector: str, using: str = "css selector") -> dict[str, str]:
        return self.request(
            "POST",
            self.path(f"/element/{parent[ELEMENT]}/element"),
            {"using": using, "value": selector},
        )

    def click(self, element: dict[str, str]) -> None:
        self.execute(
            "arguments[0].scrollIntoView({block: 'center'}); arguments[0].click();",
            [element],
        )

    def displayed(self, element: dict[str, str]) -> bool:
        return self.request("GET", self.path(f"/element/{element[ELEMENT]}/displayed"))

    def execute(self, script: str, arguments: list[Any] | None = None) -> Any:
        return self.request(
            "POST", self.path("/execute/sync"), {"script": script, "args": arguments or []}
        )

    def body(self) -> str:
        return self.execute("return document.body.innerText")

    def set_value(self, element: dict[str, str], value: str) -> None:
        self.execute(
            """
            const element = arguments[0], value = arguments[1];
            const descriptor = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(element), "value");
            if (descriptor?.set) descriptor.set.call(element, value); else element.value = value;
            element.dispatchEvent(new Event("input", {bubbles: true}));
            element.dispatchEvent(new Event("change", {bubbles: true}));
            """,
            [element, value],
        )

    def set_file(self, element: dict[str, str], path: Path) -> None:
        value = str(path)
        self.request(
            "POST",
            self.path(f"/element/{element[ELEMENT]}/value"),
            {"text": value, "value": list(value)},
        )

    def wait(self, predicate: Callable[[], Any], description: str, timeout: int = 60) -> Any:
        deadline = time.monotonic() + timeout
        last: Any = None
        while time.monotonic() < deadline:
            try:
                last = predicate()
                if last:
                    return last
            except Exception as error:  # DOM replacement while rendering is expected.
                last = error
            time.sleep(0.2)
        try:
            logs = self.request("POST", self.path("/log"), {"type": "browser"})
        except Exception as error:
            logs = [f"browser log unavailable: {error}"]
        raise RuntimeError(
            f"timed out waiting for {description}; last={last!r}; "
            f"body={self.body()[:1800]!r}; browser_log={logs!r}"
        )

    def wait_text(self, text: str, timeout: int = 60) -> None:
        self.wait(lambda: text in self.body(), repr(text), timeout)

    def click_text(self, text: str, tag: str = "button") -> None:
        encoded = json.dumps(text)
        for element in self.elements(f"//{tag}[normalize-space()={encoded}]", "xpath"):
            if self.displayed(element):
                self.click(element)
                return
        raise RuntimeError(f"no visible {tag} has text {text!r}")

    def control(self, form_selector: str, label: str, tag: str = "input") -> dict[str, str]:
        form = self.element(form_selector)
        encoded = json.dumps(label)
        return self.child(form, f".//label[.//span[normalize-space()={encoded}]]//{tag}", "xpath")

    def submit(self, form_selector: str) -> None:
        result = self.execute(
            """
            const form = arguments[0];
            const invalid = [...form.elements].filter(element => !element.checkValidity())
              .map(element => ({name: element.name, type: element.type, value: element.value}));
            let submitted = false;
            form.addEventListener("submit", () => { submitted = true; }, {once: true});
            if (invalid.length === 0) form.requestSubmit();
            return {
              invalid,
              submitted,
              values: [...form.elements].map(element => ({type: element.type, value: element.value})),
            };
            """,
            [self.element(form_selector)],
        )
        if result["invalid"]:
            raise RuntimeError(f"form {form_selector} is invalid: {result['invalid']}")
        if not result["submitted"]:
            raise RuntimeError(f"form {form_selector} did not emit submit: {result['values']}")


def check_headers(url: str) -> None:
    with urllib.request.urlopen(url, timeout=10) as response:
        headers = {name.casefold(): value for name, value in response.headers.items()}
    csp = headers.get("content-security-policy", "")
    required = ("default-src 'self'", "worker-src 'self'", "'wasm-unsafe-eval'", "'sha256-")
    if any(item not in csp for item in required) or "'unsafe-inline'" in csp or " 'unsafe-eval'" in csp:
        raise RuntimeError(f"strict generated CSP is missing or unsafe: {csp!r}")
    for name in (
        "x-content-type-options",
        "referrer-policy",
        "permissions-policy",
        "cross-origin-opener-policy",
        "cross-origin-resource-policy",
    ):
        if name not in headers:
            raise RuntimeError(f"missing security header {name}")
    print("PASS headers: generated CSP hash and static security headers are active")


def make_catalog(root: Path) -> tuple[Path, Path, Path]:
    cities = root / "cities500.zip"
    row = "\t".join(
        [
            "999001", "São José", "Sao Jose", "", "40", "-74", "P", "PPL", "US", "",
            "NY", "001", "", "", "1000", "", "10", "America/New_York", "2026-01-01",
        ]
    ) + "\n"
    with zipfile.ZipFile(cities, "w", compression=zipfile.ZIP_STORED) as archive:
        archive.writestr("cities500.txt", row)
    admin1 = root / "admin1CodesASCII.txt"
    admin1.write_text("US.NY\tNew York\tNew York\t1\n", encoding="utf-8")
    admin2 = root / "admin2Codes.txt"
    admin2.write_text("US.NY.001\tExample County\tExample County\t1\n", encoding="utf-8")
    return cities, admin1, admin2


def fill_chart(driver: Driver, label: str, role: str, date: str) -> None:
    form = "form.new-chart-editor"
    driver.set_value(driver.control(form, "Chart name"), label)
    driver.set_value(driver.control(form, "Role", "select"), role)
    driver.set_value(driver.control(form, "Date"), date)
    driver.set_value(driver.control(form, "Time"), "12:00:00")
    driver.set_value(driver.control(form, "IANA time zone"), "America/New_York")
    driver.click_text("Create chart")
    driver.wait_text(label)


def run_acceptance(driver: Driver, launch_url: str, downloads: Path) -> None:
    driver.request("POST", driver.path("/url"), {"url": launch_url})
    driver.wait_text("Browser-local studio ready.")
    current = driver.execute("return location.href")
    if "#token=" in current or current != launch_url:
        raise RuntimeError(f"application did not use the stable token-free origin: {current}")
    print("PASS launch: open static application loaded without authentication")

    driver.click_text("Files", "a")
    driver.wait_text("Exports are your backups.")
    driver.click_text("New scratch")
    driver.wait_text("Scratch")
    driver.click_text("Settings", "a")
    # innerText reflects the eyebrow's rendered uppercase styling.
    driver.wait_text("STUDIO PREFERENCES")
    person_form = "form.person-editor"
    driver.set_value(driver.control(person_form, "Display name"), "Fictional Person")
    driver.click_text("Add person")
    driver.wait_text("Fictional Person")
    warned = driver.execute(
        "const event = new Event('beforeunload', {cancelable:true}); window.dispatchEvent(event); return event.defaultPrevented;"
    )
    if not warned:
        raise RuntimeError("dirty scratch did not install a page-exit warning")
    print("PASS scratch: volatile work becomes dirty and warns before page exit")

    location_form = "form.location-editor"
    for label, value in (
        ("Location name", "Fictional Harbor"),
        ("Country", "US"),
        ("IANA time zone", "America/New_York"),
        ("Latitude", "40.0"),
        ("Longitude", "-75.0"),
    ):
        driver.set_value(driver.control(location_form, label), value)
    driver.submit(location_form)
    driver.wait_text("Fictional Harbor")

    with tempfile.TemporaryDirectory(prefix="oracle-geonames-") as fixture_dir:
        paths = make_catalog(Path(fixture_dir))
        catalog_form = "form.catalog-upload"
        for label, path in zip(
            ("cities500.zip", "admin1CodesASCII.txt", "admin2Codes.txt"), paths, strict=True
        ):
            driver.set_file(driver.control(catalog_form, label), path)
        driver.click_text("Install local catalog")
        driver.wait_text("Installed 1 GeoNames places.")
        driver.set_value(driver.element("form.inline-search input"), "sao jose")
        driver.click_text("Search locally")
        driver.wait_text("São José")
    print("PASS locations: manual fallback and uploaded Unicode GeoNames search run in the worker")

    fill_chart(driver, "Fictional natal", "natal", "2000-01-15")
    fill_chart(driver, "Fictional transit", "transit", "2026-08-17")
    driver.execute("document.querySelector('details.advanced').open = true")
    comparison = "form.comparison-editor"
    driver.set_value(driver.control(comparison, "Preset name"), "Fictional comparison")
    driver.set_value(driver.control(comparison, "Inner chart", "select"), "fictional-natal")
    driver.set_value(driver.control(comparison, "Outer chart", "select"), "fictional-transit")
    driver.click_text("Save comparison preset")
    driver.wait_text("Fictional comparison")
    driver.click_text("Workbench", "a")
    transit_card = driver.element(
        "//article[.//strong[normalize-space()='Fictional transit']]", "xpath"
    )
    driver.click(driver.child(transit_card, ".//button[normalize-space()='Use as Outer']", "xpath"))
    driver.wait(
        lambda: driver.execute(
            "return document.querySelector('.outer-meta').textContent.includes('Fictional transit') && Boolean(document.querySelector('#oracle-transit-biwheel'))"
        ),
        "Moshier workbench wheel",
        timeout=120,
    )
    controller = driver.execute(
        """
        return [...document.querySelectorAll('.time-column')].map(column =>
          [...column.children].map(item => item.textContent.trim()));
        """
    )
    expected_labels = ["1m", "10m", "1h", "1d", "5d", "30d", "1y", "10y"]
    if len(controller) != 8 or any(
        column != [">>", ">", label, "<", "<<"]
        for column, label in zip(controller, expected_labels, strict=True)
    ):
        raise RuntimeError(f"time controller does not preserve the exact eight columns: {controller}")
    day_forward = driver.element(".time-column:nth-child(4) button:nth-child(2)")
    driver.click(day_forward)
    driver.wait(
        lambda: "2026-08-18" in driver.execute(
            "return document.querySelector('.outer-meta').textContent"
        ),
        "one-day Moshier preview",
        timeout=120,
    )
    pluto = driver.element(".filters-module .filter-grid label:nth-child(10) input")
    driver.click(pluto)
    driver.wait(
        lambda: driver.execute(
            "return document.querySelectorAll('[data-interaction=point][data-point-id=Pluto]').length === 0"
        ),
        "session point filter",
    )
    metadata = driver.execute(
        """
        const point = document.querySelector('[data-interaction=point]');
        const aspect = document.querySelector('[data-interaction=aspect]');
        return {point: Boolean(point && point.tabIndex === 0 && point.getAttribute('aria-label')),
                aspect: Boolean(aspect && aspect.tabIndex === 0 && aspect.dataset.natalId && aspect.dataset.transitId)};
        """
    )
    if metadata != {"point": True, "aspect": True}:
        raise RuntimeError(f"SVG interaction metadata is incomplete: {metadata}")
    print("PASS chart domain: real Moshier wheel, exact time controller, filters, and SVG metadata")

    driver.click_text("Files", "a")
    scratch_form = "form.save-scratch"
    driver.set_value(driver.control(scratch_form, "Public vault title"), "Fictional Portable Studio")
    driver.set_value(driver.control(scratch_form, "Password"), "fictional browser password")
    driver.click_text("Save encrypted vault")
    driver.wait_text("Fictional Portable Studio", timeout=90)
    driver.wait_text("ACTIVE", timeout=90)

    driver.execute("location.reload()")
    driver.wait_text("Browser-local studio ready.")
    driver.wait_text("Fictional Portable Studio")
    driver.wait_text("LOCKED")
    card = driver.element("//article[.//h2[normalize-space()='Fictional Portable Studio']]", "xpath")
    password = driver.child(card, ".//label[.//span[normalize-space()='Password']]//input", "xpath")
    driver.set_value(password, "fictional browser password")
    driver.click(driver.child(card, ".//button[normalize-space()='Unlock']", "xpath"))
    driver.click_text("Settings", "a")
    driver.wait_text("Fictional Person", timeout=90)
    driver.click_text("Files", "a")
    card = driver.element("//article[.//h2[normalize-space()='Fictional Portable Studio']]", "xpath")
    driver.click(driver.child(card, ".//button[normalize-space()='Lock']", "xpath"))
    driver.wait_text("LOCKED")
    card = driver.element("//article[.//h2[normalize-space()='Fictional Portable Studio']]", "xpath")
    driver.click(driver.child(card, ".//button[normalize-space()='Export']", "xpath"))
    driver.wait_text("Downloaded fictional-portable-studio.oracle-vault.")
    driver.wait(lambda: any(downloads.glob("*.oracle-vault")), "portable vault download")
    exported = next(downloads.glob("*.oracle-vault"))
    if exported.stat().st_size < 100:
        raise RuntimeError("portable vault export is unexpectedly small")
    print("PASS vault: IndexedDB reload, unlock, export, and independent lock succeed")

    metrics = driver.execute(
        """
        const external = performance.getEntriesByType('resource').map(item => item.name).filter(
          name => !name.startsWith(location.origin) && !name.startsWith('blob:') && !name.startsWith('data:'));
        return {external, overflow: document.documentElement.scrollWidth > document.documentElement.clientWidth};
        """
    )
    if metrics != {"external": [], "overflow": False}:
        raise RuntimeError(f"desktop resource/layout check failed: {metrics}")
    driver.request("POST", driver.path("/window/rect"), {"width": 390, "height": 844, "x": 0, "y": 0})
    mobile_overflow = driver.execute(
        "return document.documentElement.scrollWidth > document.documentElement.clientWidth"
    )
    if mobile_overflow:
        raise RuntimeError("mobile viewport has horizontal overflow")
    driver.click_text("Workbench", "a")
    driver.click_text("Controls")
    if len(driver.elements(".right-sidebar.drawer-open .time-column")) != 8:
        raise RuntimeError("mobile Controls drawer lost one or more time columns")
    accessibility = driver.request(
        "POST", driver.path("/goog/cdp/execute"), {"cmd": "Accessibility.getFullAXTree", "params": {}}
    )
    names = {
        node.get("name", {}).get("value")
        for node in accessibility.get("nodes", [])
        if node.get("name")
    }
    if "Application views" not in names:
        raise RuntimeError("accessibility tree is missing the named navigation landmark")
    focus = driver.execute(
        "const main=document.querySelector('#workbench'); main.focus(); return document.activeElement===main && main.tabIndex===-1;"
    )
    if not focus:
        raise RuntimeError("main focus target is unavailable")
    print("PASS browser: responsive layout, focus target, accessibility landmark, and no external requests")


def wait_for_driver(url: str) -> None:
    for _ in range(150):
        try:
            with urllib.request.urlopen(url + "/status", timeout=1) as response:
                if json.load(response).get("value", {}).get("ready"):
                    return
        except (OSError, ValueError):
            pass
        time.sleep(0.1)
    raise RuntimeError("ChromeDriver did not become ready")


def main() -> int:
    launch_url = os.environ.get("ORACLE_STUDIO_URL", "http://127.0.0.1:8080/")
    if launch_url != "http://127.0.0.1:8080/":
        raise RuntimeError("acceptance requires the stable loopback origin")
    check_headers(launch_url)
    downloads = Path("/tmp/oracle-downloads")
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
        with tempfile.TemporaryDirectory(prefix="oracle-chrome-") as profile:
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
                                    "--headless=new", "--no-sandbox", "--disable-gpu", "--disable-dev-shm-usage",
                                    "--disable-background-networking", "--disable-component-update", "--disable-sync",
                                    "--metrics-recording-only", "--no-first-run", "--no-default-browser-check",
                                    "--password-store=basic", "--use-mock-keychain",
                                    "--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE 127.0.0.1",
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
