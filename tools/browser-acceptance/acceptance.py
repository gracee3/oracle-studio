#!/usr/bin/env python3
"""One-use browser acceptance for the Oracle Studio Rust/WASM application."""

from __future__ import annotations

import base64
import json
import os
import re
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any, Callable

ELEMENT_KEY = "element-6066-11e4-a52e-4f735466cecf"
LAUNCH_PATTERN = re.compile(r"http://127\.0\.0\.1:\d+/#token=[0-9a-f]{64}")


class WebDriver:
    def __init__(self, base_url: str) -> None:
        self.base_url = base_url
        self.session_id: str | None = None

    def request(
        self,
        method: str,
        path: str,
        payload: dict[str, Any] | None = None,
        timeout: int = 30,
    ) -> Any:
        data = None if payload is None else json.dumps(payload).encode()
        request = urllib.request.Request(
            self.base_url + path,
            data=data,
            method=method,
            headers={"Content-Type": "application/json"},
        )
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                result = json.load(response)
        except urllib.error.HTTPError as error:
            response = error.read().decode(errors="replace")
            raise RuntimeError(
                f"WebDriver {method} {path} returned HTTP {error.code}: {response[:1000]}"
            ) from error
        value = result.get("value")
        if isinstance(value, dict) and value.get("error"):
            raise RuntimeError(
                f"WebDriver {value['error']}: {value.get('message', '')}"
            )
        return value

    def session_path(self, suffix: str = "") -> str:
        if self.session_id is None:
            raise RuntimeError("WebDriver session is not open")
        return f"/session/{self.session_id}{suffix}"

    def elements(self, selector: str, using: str = "css selector") -> list[dict[str, str]]:
        return self.request(
            "POST",
            self.session_path("/elements"),
            {"using": using, "value": selector},
        )

    def element(self, selector: str, using: str = "css selector") -> dict[str, str]:
        return self.request(
            "POST",
            self.session_path("/element"),
            {"using": using, "value": selector},
        )

    def child_elements(
        self,
        parent: dict[str, str],
        selector: str,
        using: str = "css selector",
    ) -> list[dict[str, str]]:
        return self.request(
            "POST",
            self.session_path(f"/element/{parent[ELEMENT_KEY]}/elements"),
            {"using": using, "value": selector},
        )

    def click(self, element: dict[str, str]) -> None:
        self.request(
            "POST", self.session_path(f"/element/{element[ELEMENT_KEY]}/click"), {}
        )

    def displayed(self, element: dict[str, str]) -> bool:
        return self.request(
            "GET", self.session_path(f"/element/{element[ELEMENT_KEY]}/displayed")
        )

    def selected(self, element: dict[str, str]) -> bool:
        return self.request(
            "GET", self.session_path(f"/element/{element[ELEMENT_KEY]}/selected")
        )

    def execute(self, script: str, arguments: list[Any] | None = None) -> Any:
        return self.request(
            "POST",
            self.session_path("/execute/sync"),
            {"script": script, "args": arguments or []},
        )

    def body_text(self) -> str:
        return self.execute("return document.body.innerText")

    def set_value(self, element: dict[str, str], value: str) -> None:
        self.execute(
            """
            const element = arguments[0], value = arguments[1];
            const descriptor = Object.getOwnPropertyDescriptor(
                Object.getPrototypeOf(element), "value"
            );
            if (descriptor && descriptor.set) descriptor.set.call(element, value);
            else element.value = value;
            element.dispatchEvent(new Event("input", {bubbles: true}));
            element.dispatchEvent(new Event("change", {bubbles: true}));
            """,
            [element, value],
        )

    def choose_text(self, element: dict[str, str], text: str) -> None:
        found = self.execute(
            """
            const element = arguments[0], wanted = arguments[1];
            const option = [...element.options].find(
                candidate => candidate.textContent.includes(wanted)
            );
            if (!option) return false;
            element.value = option.value;
            element.dispatchEvent(new Event("input", {bubbles: true}));
            element.dispatchEvent(new Event("change", {bubbles: true}));
            return true;
            """,
            [element, text],
        )
        if not found:
            raise RuntimeError(f"select has no option containing {text!r}")

    def wait_for(
        self,
        predicate: Callable[[], Any],
        description: str,
        timeout: int = 40,
    ) -> Any:
        deadline = time.monotonic() + timeout
        last: Any = None
        while time.monotonic() < deadline:
            try:
                last = predicate()
                if last:
                    return last
            except Exception as error:  # transient DOM replacement is expected
                last = error
            time.sleep(0.2)
        body = self.body_text()[:1500] if self.session_id else ""
        raise RuntimeError(
            f"timed out waiting for {description}; last={last!r}; body={body!r}"
        )

    def wait_text(self, text: str, timeout: int = 40) -> None:
        self.wait_for(lambda: text in self.body_text(), repr(text), timeout)

    def xpath(self, expression: str) -> dict[str, str]:
        return self.element(expression, "xpath")

    def click_text(self, text: str, tag: str = "*") -> None:
        xpath_text = json.dumps(text)
        for candidate in self.elements(
            f"//{tag}[normalize-space()={xpath_text}]", "xpath"
        ):
            if self.displayed(candidate):
                self.click(candidate)
                return
        raise RuntimeError(f"no visible {tag} has text {text!r}")

    def labeled_control(self, label: str, tag: str = "input") -> dict[str, str]:
        xpath_text = json.dumps(label)
        return self.xpath(
            f"//label[.//span[normalize-space()={xpath_text}]]//{tag}"
        )

    def screenshot(self, destination: Path) -> None:
        encoded = self.request("GET", self.session_path("/screenshot"))
        destination.write_bytes(base64.b64decode(encoded))


def wait_for_driver(base_url: str, timeout: int = 15) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            with urllib.request.urlopen(base_url + "/status", timeout=1) as response:
                if json.load(response).get("value", {}).get("ready"):
                    return
        except (OSError, ValueError):
            pass
        time.sleep(0.1)
    raise RuntimeError("ChromeDriver did not become ready")


def save_manual_location(
    driver: WebDriver,
    record_id: str,
    label: str,
    administrative_names: str,
    latitude: str,
    longitude: str,
) -> None:
    fields = {
        "Record ID": record_id,
        "Label": label,
        "Administrative names (comma-separated)": administrative_names,
        "Country code": "US",
        "IANA time zone": "America/New_York",
        "Latitude": latitude,
        "Longitude": longitude,
    }
    for field_label, value in fields.items():
        driver.set_value(driver.labeled_control(field_label), value)
    driver.click_text("Save manual location", "button")
    driver.wait_text("Saved the manual location.")
    driver.wait_for(
        lambda: label in driver.body_text(), f"saved location {label!r}"
    )


def save_and_calculate_chart(
    driver: WebDriver,
    *,
    label: str,
    role: str,
    person: str | None,
    local_date: str,
    local_time: str,
    expected_instant: str,
    expected_zone: str,
    location: str,
    default_natal: bool = False,
) -> None:
    driver.set_value(driver.labeled_control("Chart label"), label)
    driver.choose_text(driver.labeled_control("Role", "select"), role)
    if person is not None:
        driver.choose_text(
            driver.labeled_control("Person (optional)", "select"), person
        )
    driver.set_value(driver.labeled_control("Local date"), local_date)
    driver.set_value(driver.labeled_control("Local time"), local_time)
    driver.set_value(
        driver.labeled_control("IANA time zone"), "America/New_York"
    )
    default = driver.element("label.check-line input[type=checkbox]")
    if default_natal and not driver.selected(default):
        driver.click(default)
    driver.click_text("Save and resolve time", "button")
    driver.wait_text(expected_instant)
    driver.wait_text(expected_zone)
    location_select = driver.xpath(
        "//label[.//span[normalize-space()='Saved location snapshot']]//select"
    )
    driver.choose_text(location_select, location)
    driver.click_text("Calculate this instant", "button")
    driver.wait_text("Calculation saved. Earlier calculations remain unchanged.")
    driver.wait_for(
        lambda: location in driver.body_text()
        and expected_instant in driver.body_text(),
        f"calculation history for {label!r}",
    )


def run_acceptance(driver: WebDriver, launch_url: str, vault_path: str) -> None:
    driver.request("POST", driver.session_path("/url"), {"url": launch_url})
    driver.wait_text("Open a vault")
    print("PASS launch: Rust/WASM UI loaded from the authenticated loopback host")

    driver.click_text("Open a vault", "a")
    driver.wait_text("Open your studio")
    forms = driver.elements("form.vault-form")
    if len(forms) != 2:
        raise RuntimeError(f"expected two vault forms, found {len(forms)}")
    fields = driver.child_elements(forms[1], "input")
    driver.set_value(fields[0], vault_path)
    password = base64.urlsafe_b64encode(os.urandom(48)).decode()
    driver.set_value(fields[1], password)
    password = ""
    driver.click(driver.child_elements(forms[1], "button[type=submit]")[0])
    driver.wait_text("Vault created and unlocked.")
    driver.wait_text("acceptance.oracle")
    driver.wait_text("Lock")
    print("PASS vault: one-use encrypted schema-v3 vault created and unlocked")

    driver.click_text("People", "a")
    driver.wait_text("Person details")
    person_form = driver.element("form.studio-form")
    person_inputs = driver.child_elements(person_form, "input")
    driver.set_value(person_inputs[0], "ada_example")
    driver.set_value(person_inputs[1], "Ada Example")
    driver.click(driver.child_elements(person_form, "button[type=submit]")[0])
    driver.wait_text("Person saved.")
    print("PASS people: fictional person record created through the encrypted UI")

    driver.click_text("Locations", "a")
    driver.wait_text("Manual location")
    save_manual_location(
        driver,
        "fictional_harbor",
        "Fictional Harbor",
        "Example County, Pennsylvania",
        "40.0000",
        "-75.0000",
    )
    save_manual_location(
        driver,
        "fictional_capital",
        "Fictional Capital",
        "Example District",
        "38.9000",
        "-77.0000",
    )
    print("PASS locations: two encrypted manual snapshots saved without network lookup")

    driver.click_text("New chart", "a")
    driver.wait_for(
        lambda: len(driver.elements("form.chart-editor")) == 1,
        "natal chart editor",
    )
    save_and_calculate_chart(
        driver,
        label="Ada example natal",
        role="Natal",
        person="Ada Example",
        local_date="2000-01-15",
        local_time="12:30:00",
        expected_instant="2000-01-15T17:30:00Z",
        expected_zone="EST UTC-05:00",
        location="Fictional Harbor",
        default_natal=True,
    )
    print("PASS natal: local civil input resolved explicitly and calculated")

    driver.click_text("Oracle Studio", "a")
    driver.wait_text("A private studio for natal and transit work.")
    driver.click_text("New chart", "a")
    driver.wait_for(
        lambda: len(driver.elements("form.chart-editor")) == 1,
        "transit chart editor",
    )
    save_and_calculate_chart(
        driver,
        label="Fictional transit",
        role="Transit",
        person=None,
        local_date="2026-08-17",
        local_time="16:20:00",
        expected_instant="2026-08-17T20:20:00Z",
        expected_zone="EDT UTC-04:00",
        location="Fictional Capital",
    )
    print("PASS transit: local civil input resolved explicitly and calculated")

    driver.click_text("Workspace", "a")
    driver.wait_for(
        lambda: len(driver.elements("form.comparison-builder select")) >= 3,
        "comparison builder",
    )
    driver.set_value(
        driver.labeled_control("Preset label"), "Fictional natal + transit"
    )
    builder_selects = driver.elements("form.comparison-builder select")
    driver.choose_text(builder_selects[0], "Ada example natal")
    driver.choose_text(builder_selects[1], "Fictional transit")
    driver.click_text("Save, calculate, and open", "button")
    driver.wait_text("Active comparison", timeout=60)
    driver.wait_for(
        lambda: len(driver.elements("#oracle-transit-biwheel")) == 1,
        "rendered biwheel",
        timeout=60,
    )

    information = driver.body_text().casefold()
    for expected in (
        "Fictional natal + transit",
        "Inner · natal",
        "Outer · transit",
        "2000-01-15",
        "12:30:00",
        "EST UTC-05:00",
        "Fictional Harbor",
        "2026-08-17",
        "16:20:00",
        "EDT UTC-04:00",
        "Fictional Capital",
        "Tropical",
        "Placidus",
    ):
        if expected.casefold() not in information:
            raise RuntimeError(f"workspace information is missing {expected!r}")

    metrics = driver.execute(
        """
        const svg = document.querySelector("#oracle-transit-biwheel");
        const resources = performance.getEntriesByType("resource").map(entry => entry.name);
        return {
            natal: svg.querySelectorAll("#natal-layer .chart-point").length,
            transit: svg.querySelectorAll("#transit-layer .chart-point").length,
            cusps: svg.querySelectorAll('[data-role="cusp-label"]').length,
            natalAsc: svg.querySelectorAll("#natal-point-ascendant").length,
            natalMc: svg.querySelectorAll("#natal-point-midheaven").length,
            transitAsc: svg.querySelectorAll("#transit-point-ascendant").length,
            transitMc: svg.querySelectorAll("#transit-point-midheaven").length,
            signs: svg.querySelectorAll('[data-role="sign"]').length,
            cuspSigns: svg.querySelectorAll('[data-role="cusp-sign"]').length,
            orientation: svg.getAttribute("data-orientation"),
            astronomicon: svg.outerHTML.includes("font-family:Astronomicon")
                && svg.outerHTML.includes("data:font/ttf;base64,"),
            external: resources.filter(
                resource => !resource.startsWith(location.origin)
                    && !resource.startsWith("data:")
            ),
            overflow: document.documentElement.scrollWidth
                > document.documentElement.clientWidth,
        };
        """
    )
    expected_metrics = {
        "natal": 10,
        "transit": 12,
        "cusps": 12,
        "natalAsc": 0,
        "natalMc": 0,
        "transitAsc": 1,
        "transitMc": 1,
        "signs": 22,
        "cuspSigns": 12,
        "orientation": "ascendant-left",
        "astronomicon": True,
        "external": [],
        "overflow": False,
    }
    for name, expected in expected_metrics.items():
        if metrics.get(name) != expected:
            raise RuntimeError(
                f"unexpected desktop chart metric {name}: "
                f"got {metrics.get(name)!r}, expected {expected!r}"
            )
    print(
        "PASS biwheel: exact lane/cusp populations, structural angles, "
        "embedded Astronomicon, and offline resources"
    )

    review_dir = os.environ.get("ORACLE_STUDIO_REVIEW_DIR")
    if review_dir:
        review = Path(review_dir)
        review.mkdir(parents=True, exist_ok=True)
        driver.screenshot(review / "desktop.png")

    driver.request(
        "POST",
        driver.session_path("/window/rect"),
        {"width": 390, "height": 844, "x": 0, "y": 0},
    )
    time.sleep(0.5)
    mobile = driver.execute(
        """
        return {
            scrollWidth: document.documentElement.scrollWidth,
            clientWidth: document.documentElement.clientWidth,
            svgVisible: document.querySelector("#oracle-transit-biwheel")
                ?.getBoundingClientRect().width > 0,
            informationCards: document.querySelectorAll(".chart-information").length,
        };
        """
    )
    if (
        mobile["scrollWidth"] > mobile["clientWidth"]
        or not mobile["svgVisible"]
        or mobile["informationCards"] != 2
    ):
        raise RuntimeError(f"mobile responsive check failed: {mobile}")
    if review_dir:
        driver.screenshot(Path(review_dir) / "mobile.png")
    print("PASS responsive: 390x844 retains headers and SVG without page overflow")

    accessibility = driver.request(
        "POST",
        driver.session_path("/goog/cdp/execute"),
        {"cmd": "Accessibility.getFullAXTree", "params": {}},
    )
    names = {
        node.get("name", {}).get("value")
        for node in accessibility.get("nodes", [])
        if node.get("name")
    }
    for expected in ("Studio sections", "Transit biwheel"):
        if not any(expected in name for name in names if isinstance(name, str)):
            raise RuntimeError(f"accessibility tree is missing {expected!r}")
    focus = driver.execute(
        """
        const main = document.querySelector("#main-content");
        main.focus();
        return {
            focused: document.activeElement === main,
            tabindex: main.getAttribute("tabindex"),
        };
        """
    )
    if focus != {"focused": True, "tabindex": "-1"}:
        raise RuntimeError(f"main focus target is invalid: {focus}")
    print("PASS accessibility: named landmarks/chart and programmatic main focus target")


def main() -> int:
    launch_url = sys.stdin.readline().strip()
    if not LAUNCH_PATTERN.fullmatch(launch_url):
        print("expected one authenticated 127.0.0.1 launch URL on stdin", file=sys.stderr)
        return 2
    vault_path = os.environ.get("ORACLE_STUDIO_VAULT_PATH")
    if not vault_path:
        print("ORACLE_STUDIO_VAULT_PATH is required", file=sys.stderr)
        return 2

    chrome = os.environ.get("CHROME_BIN", "google-chrome")
    chromedriver = os.environ.get("CHROMEDRIVER_BIN", "chromedriver")
    Path(os.environ.get("HOME", "/tmp")).mkdir(parents=True, exist_ok=True)

    with socket.socket() as port_socket:
        port_socket.bind(("127.0.0.1", 0))
        driver_port = port_socket.getsockname()[1]
    driver_url = f"http://127.0.0.1:{driver_port}"
    process = subprocess.Popen(
        [
            chromedriver,
            f"--port={driver_port}",
            "--allowed-ips=127.0.0.1",
            "--log-level=WARNING",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    driver = WebDriver(driver_url)
    try:
        wait_for_driver(driver_url)
        with tempfile.TemporaryDirectory(prefix="oracle-studio-chrome-") as profile:
            session = driver.request(
                "POST",
                "/session",
                {
                    "capabilities": {
                        "alwaysMatch": {
                            "browserName": "chrome",
                            "goog:chromeOptions": {
                                "binary": chrome,
                                "args": [
                                    "--headless=new",
                                    "--no-sandbox",
                                    "--disable-gpu",
                                    "--disable-dev-shm-usage",
                                    "--disable-background-networking",
                                    "--disable-component-update",
                                    "--disable-sync",
                                    "--metrics-recording-only",
                                    "--no-first-run",
                                    "--no-default-browser-check",
                                    "--password-store=basic",
                                    "--use-mock-keychain",
                                    "--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE 127.0.0.1",
                                    "--window-size=1440,1200",
                                    f"--user-data-dir={profile}",
                                ],
                            },
                        }
                    }
                },
                timeout=45,
            )
            driver.session_id = session.get("sessionId")
            if not driver.session_id:
                raise RuntimeError("ChromeDriver response has no session id")
            run_acceptance(driver, launch_url, vault_path)
    finally:
        if driver.session_id:
            try:
                driver.request("DELETE", driver.session_path())
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
