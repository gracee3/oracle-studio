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
import urllib.parse
import urllib.request
import zipfile
from pathlib import Path
from typing import Any, Callable

ELEMENT = "element-6066-11e4-a52e-4f735466cecf"


def validate_loopback_url(url: str, product: str) -> None:
    parsed = urllib.parse.urlsplit(url)
    if (
        parsed.scheme != "http"
        or parsed.hostname != "127.0.0.1"
        or parsed.port is None
        or parsed.path != "/"
        or parsed.query
        or parsed.fragment
    ):
        raise RuntimeError(f"{product} acceptance requires a loopback HTTP origin")


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
    required = (
        "default-src 'self'",
        "worker-src 'self'",
        "'wasm-unsafe-eval'",
        "style-src 'self' 'sha256-",
        "script-src 'self' 'wasm-unsafe-eval' 'sha256-",
    )
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
    driver.request(
        "POST", driver.path("/window/rect"), {"width": 1440, "height": 900, "x": 0, "y": 0}
    )
    current = driver.execute("return location.href")
    if "#token=" in current or current != launch_url:
        raise RuntimeError(f"application did not use the stable token-free origin: {current}")
    print("PASS launch: open static application loaded without authentication")

    initial_theme = driver.execute(
        "return {theme: document.documentElement.dataset.theme, surface: getComputedStyle(document.documentElement).getPropertyValue('--surface').trim()}"
    )
    if initial_theme["theme"] not in ("light", "dark") or not initial_theme["surface"]:
        raise RuntimeError(f"prepaint theme bootstrap did not resolve a complete theme: {initial_theme}")
    driver.click(driver.element(".theme-toggle"))
    toggled_theme = driver.execute(
        "return {theme: document.documentElement.dataset.theme, saved: localStorage.getItem('oracle-studio.theme.v1'), surface: getComputedStyle(document.documentElement).getPropertyValue('--surface').trim()}"
    )
    if (
        toggled_theme["theme"] == initial_theme["theme"]
        or toggled_theme["saved"] != toggled_theme["theme"]
        or toggled_theme["surface"] == initial_theme["surface"]
    ):
        raise RuntimeError(
            f"theme toggle did not change and persist the semantic scheme: {initial_theme} -> {toggled_theme}"
        )
    driver.execute("location.reload()")
    driver.wait_text("Browser-local studio ready.")
    persisted_theme = driver.execute("return document.documentElement.dataset.theme")
    if persisted_theme != toggled_theme["theme"]:
        raise RuntimeError(f"prepaint bootstrap did not restore {toggled_theme['theme']}: {persisted_theme}")
    driver.click_text("Settings", "a")
    driver.wait_text("LOCAL APPEARANCE")
    driver.click_text("Reset to system")
    reset_theme = driver.execute(
        "return {theme: document.documentElement.dataset.theme, saved: localStorage.getItem('oracle-studio.theme.v1')}"
    )
    if reset_theme["theme"] not in ("light", "dark") or reset_theme["saved"] is not None:
        raise RuntimeError(f"system theme reset did not remove the explicit preference: {reset_theme}")
    print("PASS themes: prepaint selection, semantic toggle, persistence, and system reset succeed")

    driver.click_text("Files", "a")
    driver.wait_text("Exports are your backups.")
    if driver.elements(".demo-controls") or "oracle-demo" in driver.body():
        raise RuntimeError("ordinary production build exposed the opt-in demo loader")
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
        driver.set_value(
            driver.element(".catalog-controls form.inline-search input"), "sao jose"
        )
        driver.click_text("Search locally")
        driver.wait_text("São José")
    print("PASS locations: manual fallback and uploaded Unicode GeoNames search run in the worker")

    built_in_aspect_sets = driver.execute(
        "return [...document.querySelectorAll('.aspect-set-settings select option')].map(option => option.textContent.trim())"
    )
    if built_in_aspect_sets[:4] != [
        "Tight · built-in",
        "Standard · built-in",
        "Synastry · built-in",
        "Synwide · built-in",
    ]:
        raise RuntimeError(f"reviewed aspect presets are not discoverable: {built_in_aspect_sets}")
    driver.set_value(
        driver.element("input[aria-label='New aspect-set name']"), "Fictional Focus"
    )
    driver.click_text("Create / duplicate")
    driver.wait_text("Fictional Focus")
    displayed_pluto = "//fieldset[legend[normalize-space()='Displayed points']]//label[.//span[normalize-space()='Pluto']]//input"
    aspected_pluto = "//fieldset[legend[normalize-space()='Aspected points']]//label[.//span[normalize-space()='Pluto']]//input"
    driver.click(driver.element(displayed_pluto, "xpath"))
    independent_points = driver.wait(
        lambda: driver.execute(
            "return {displayed: arguments[0].checked, aspected: arguments[1].checked}",
            [
                driver.element(displayed_pluto, "xpath"),
                driver.element(aspected_pluto, "xpath"),
            ],
        )
        == {"displayed": False, "aspected": True},
        "independent displayed and aspected point selections",
    )
    if not independent_points:
        raise RuntimeError("displayed and aspected point selections did not diverge")
    print("PASS aspect settings: built-ins, editable copy, and independent point selections succeed")

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
            "return document.querySelector('.outer-identity')?.textContent.includes('Fictional transit') && Boolean(document.querySelector('#oracle-transit-biwheel'))"
        ),
        "Moshier workbench wheel",
        timeout=120,
    )
    template_names = driver.execute(
        "return [...document.querySelectorAll('.template-list strong')].map(item => item.textContent.trim())"
    )
    expected_templates = [
        "Studio Biwheel",
        "Compact Biwheel",
        "High Contrast Biwheel",
        "Classic Single",
        "Data-forward Single",
    ]
    if template_names[:5] != expected_templates:
        raise RuntimeError(f"protected wheel templates are incomplete or reordered: {template_names}")
    driver.request("POST", driver.path("/window/rect"), {"width": 1440, "height": 900, "x": 0, "y": 0})
    desktop_rect = driver.request("GET", driver.path("/window/rect"))
    studio_metrics = driver.execute(
        "const svg=document.querySelector('#oracle-transit-biwheel'); return {mode:svg?.dataset.wheelMode, layout:svg?.dataset.wheelLayout, overflow:document.documentElement.scrollWidth>document.documentElement.clientWidth}"
    )
    if studio_metrics != {
        "mode": "biwheel",
        "layout": "balanced",
        "overflow": False,
    } or desktop_rect["width"] != 1440 or desktop_rect["height"] != 900:
        raise RuntimeError(f"Studio Biwheel desktop presentation is invalid: {studio_metrics}")

    classic = driver.element("//button[.//strong[normalize-space()='Classic Single']]", "xpath")
    driver.click(classic)
    driver.request("POST", driver.path("/window/rect"), {"width": 768, "height": 1024, "x": 0, "y": 0})
    driver.wait(
        lambda: driver.execute(
            "const svg=document.querySelector('#oracle-single-wheel'); return svg?.dataset.wheelMode==='single' && svg?.dataset.wheelLayout==='balanced' && !svg.querySelector('#transit-layer')"
        ),
        "Classic Single tablet presentation",
    )
    if driver.execute("return document.documentElement.scrollWidth > document.documentElement.clientWidth"):
        raise RuntimeError("Classic Single has horizontal overflow at 768x1024")

    driver.request("POST", driver.path("/window/rect"), {"width": 1440, "height": 900, "x": 0, "y": 0})
    data_forward = driver.element(
        "//button[.//strong[normalize-space()='Data-forward Single']]", "xpath"
    )
    driver.click(data_forward)
    driver.request("POST", driver.path("/window/rect"), {"width": 390, "height": 844, "x": 0, "y": 0})
    driver.wait(
        lambda: driver.execute(
            "const svg=document.querySelector('#oracle-single-wheel'); return svg?.dataset.wheelLayout==='data-forward' && svg.classList.contains('wheel-layout--data-forward')"
        ),
        "Data-forward Single mobile presentation",
    )
    if driver.execute("return document.documentElement.scrollWidth > document.documentElement.clientWidth"):
        raise RuntimeError("Data-forward Single has horizontal overflow at 390x844")

    driver.request("POST", driver.path("/window/rect"), {"width": 1440, "height": 900, "x": 0, "y": 0})
    before_auto_palette = driver.execute(
        "const svg=document.querySelector('#oracle-single-wheel'); return {theme:document.documentElement.dataset.theme,palette:svg?.dataset.palette}"
    )
    driver.click(driver.element(".theme-toggle"))
    driver.wait(
        lambda: driver.execute(
            "return document.querySelector('#oracle-single-wheel')?.dataset.palette !== arguments[0]",
            [before_auto_palette["palette"]],
        ),
        "theme-aware automatic wheel palette",
    )
    after_auto_palette = driver.execute(
        "const svg=document.querySelector('#oracle-single-wheel'); return {theme:document.documentElement.dataset.theme,palette:svg?.dataset.palette}"
    )
    expected_auto = {"dark": "studio-dark", "light": "paper-light"}
    if (
        before_auto_palette["palette"] != expected_auto[before_auto_palette["theme"]]
        or after_auto_palette["palette"] != expected_auto[after_auto_palette["theme"]]
    ):
        raise RuntimeError(
            f"automatic chart palette did not follow both themes: {before_auto_palette} -> {after_auto_palette}"
        )
    high_contrast = driver.element(
        "//button[.//strong[normalize-space()='High Contrast Biwheel']]", "xpath"
    )
    driver.click(high_contrast)
    driver.wait(
        lambda: driver.execute(
            "return document.querySelector('#oracle-transit-biwheel')?.dataset.palette==='high-contrast'"
        ),
        "High Contrast Biwheel",
    )
    studio = driver.element("//button[.//strong[normalize-space()='Studio Biwheel']]", "xpath")
    driver.click(studio)
    driver.wait(
        lambda: driver.execute(
            "return document.querySelector('#oracle-transit-biwheel')?.dataset.wheelLayout==='balanced'"
        ),
        "restored Studio Biwheel",
    )
    print("PASS presentation: protected single/bi-wheel templates, theme palettes, and 390/768/1440 layouts succeed")
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
    driver.click(driver.element(".time-column:nth-child(4) button:nth-child(2)"))
    driver.wait(
        lambda: "2026-08-18" in driver.execute(
            "return document.querySelector('.outer-identity').getAttribute('title')"
        ),
        "one-day Moshier preview",
        timeout=120,
    )
    if len(driver.elements(".wheel-actions button, .wheel-actions form")) != 0:
        raise RuntimeError("workbench still exposes chart persistence controls")
    if "Unsaved preview" not in driver.body() or "Review in Files" not in driver.body():
        raise RuntimeError("workbench does not expose the unsaved-preview Files handoff")
    hidden_point_policy = driver.execute(
        """
        return {
          point: document.querySelectorAll('[data-interaction=point][data-point-id=Pluto]').length,
          lines: document.querySelectorAll('[data-interaction=aspect][data-natal-id=Pluto], [data-interaction=aspect][data-transit-id=Pluto]').length,
        };
        """
    )
    if hidden_point_policy != {"point": 0, "lines": 0}:
        raise RuntimeError(f"hidden aspected point leaked onto the wheel: {hidden_point_policy}")
    neptune = driver.element(".filters-module .filter-grid label:nth-child(9) input")
    driver.click(neptune)
    driver.wait(
        lambda: driver.execute(
            "return document.querySelectorAll('[data-interaction=point][data-point-id=Neptune]').length === 0"
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

    presentation_ui = driver.execute(
        """
        const cards = [...document.querySelectorAll('.chart-identity')];
        const labels = cards.map(card => ({
          kicker: card.querySelector('.meta-kicker')?.textContent.trim(),
          heading: card.querySelector('h2')?.textContent.trim(),
          fontSize: parseFloat(getComputedStyle(card).fontSize),
          title: card.getAttribute('title'),
        }));
        const status = document.querySelector('.status-metrics');
        return {
          labels,
          status: status ? {
            values: [...status.querySelectorAll('b, span')].map(item => item.textContent.trim()),
            title: status.getAttribute('title'),
          } : null,
          zoomButtons: [...document.querySelectorAll('.zoom-controls button')]
            .map(button => button.getAttribute('aria-label')),
          zoomHint: document.querySelector('#zoom-help')?.textContent,
          width: document.querySelector('#wheel-stage').clientWidth,
        };
        """
    )
    if (
        [item["kicker"] for item in presentation_ui["labels"]] != ["Chart 1", "Chart 2"]
        or any(item["fontSize"] < 14.4 for item in presentation_ui["labels"])
        or any(not item["heading"] or "America/New_York" not in item["title"] for item in presentation_ui["labels"])
        or presentation_ui["status"] is None
        or len(presentation_ui["status"]["values"]) != 4
        or "Provider:" not in presentation_ui["status"]["title"]
        or "/home/" in presentation_ui["status"]["title"]
        or presentation_ui["zoomButtons"]
        != ["Zoom out", "Reset chart zoom", "Zoom in"]
        or "Ctrl-wheel remains browser page zoom" not in presentation_ui["zoomHint"]
    ):
        raise RuntimeError(f"chart presentation controls are incomplete: {presentation_ui}")

    driver.click(driver.element("button[aria-label='Zoom in']"))
    driver.wait(
        lambda: driver.execute(
            "return document.querySelector('#wheel-stage').dataset.zoomPercent === '110'"
        ),
        "visible zoom-in control",
    )
    driver.execute(
        "for (let i=0; i<40; i++) document.querySelector(\"button[aria-label='Zoom in']\").click()"
    )
    driver.wait(
        lambda: driver.execute(
            "return document.querySelector('#wheel-stage').dataset.zoomPercent === '300'"
        ),
        "bounded maximum chart zoom",
    )
    driver.execute(
        "for (let i=0; i<50; i++) document.querySelector(\"button[aria-label='Zoom out']\").click()"
    )
    driver.wait(
        lambda: driver.execute(
            "return document.querySelector('#wheel-stage').dataset.zoomPercent === '75'"
        ),
        "bounded minimum chart zoom",
    )
    driver.click(driver.element("button[aria-label='Reset chart zoom']"))
    driver.wait(
        lambda: driver.execute(
            "return document.querySelector('#wheel-stage').dataset.zoomPercent === '100'"
        ),
        "chart zoom reset",
    )
    wheel_result = driver.execute(
        """
        const chart = document.querySelector('.wheel-svg');
        const bounds = chart.getBoundingClientRect();
        const event = new WheelEvent('wheel', {
          bubbles: true, cancelable: true, altKey: true, deltaY: -120,
          clientX: bounds.left + bounds.width * .1,
          clientY: bounds.top + bounds.height * .1,
        });
        return {dispatchAllowed: chart.dispatchEvent(event), defaultPrevented: event.defaultPrevented};
        """
    )
    driver.wait(
        lambda: driver.execute(
            """
            const stage=document.querySelector('#wheel-stage'), chart=document.querySelector('.wheel-svg');
            return stage.dataset.zoomPercent === '110'
              && chart.classList.contains('origin-x-left')
              && chart.classList.contains('origin-y-top');
            """
        ),
        "pointer-relative Alt/Option-wheel zoom",
    )
    if wheel_result != {"dispatchAllowed": False, "defaultPrevented": True}:
        raise RuntimeError(f"Alt/Option-wheel did not suppress chart-area scrolling: {wheel_result}")
    ctrl_result = driver.execute(
        """
        const chart = document.querySelector('.wheel-svg');
        const event = new WheelEvent('wheel', {
          bubbles: true, cancelable: true, ctrlKey: true, deltaY: -120,
        });
        return {dispatchAllowed: chart.dispatchEvent(event), defaultPrevented: event.defaultPrevented,
                zoom: document.querySelector('#wheel-stage').dataset.zoomPercent};
        """
    )
    if ctrl_result != {"dispatchAllowed": True, "defaultPrevented": False, "zoom": "110"}:
        raise RuntimeError(f"Ctrl-wheel was incorrectly intercepted: {ctrl_result}")
    driver.execute(
        """
        const stage=document.querySelector('#wheel-stage');
        stage.focus();
        stage.dispatchEvent(new KeyboardEvent('keydown', {key: '-', bubbles:true, cancelable:true}));
        """
    )
    driver.wait(
        lambda: driver.execute(
            "return document.querySelector('#wheel-stage').dataset.zoomPercent === '100'"
        ),
        "focused-stage keyboard zoom",
    )
    driver.click(driver.element("button[aria-label='Zoom in']"))
    driver.click(driver.element(".template-list button.selected"))
    driver.wait(
        lambda: driver.execute(
            "return document.querySelector('#wheel-stage').dataset.zoomPercent === '100'"
        ),
        "template-change zoom reset",
    )
    if driver.execute("return document.activeElement !== document.querySelector('#wheel-stage')"):
        driver.execute("document.querySelector('#wheel-stage').focus()")
    driver.execute(
        "document.querySelector('#wheel-stage').dispatchEvent(new KeyboardEvent('keydown', {key:'+', bubbles:true, cancelable:true}))"
    )
    driver.wait(
        lambda: driver.execute(
            "return document.querySelector('#wheel-stage').dataset.zoomPercent === '110'"
        ),
        "focused-stage plus key",
    )
    driver.execute(
        "document.querySelector('#wheel-stage').dispatchEvent(new KeyboardEvent('keydown', {key:'0', bubbles:true, cancelable:true}))"
    )
    driver.wait(
        lambda: driver.execute(
            "return document.querySelector('#wheel-stage').dataset.zoomPercent === '100'"
        ),
        "focused-stage zero key",
    )

    outer_before_layout = driver.execute("return document.querySelector('.outer-identity').textContent")
    stage_width = driver.execute("return document.querySelector('#wheel-stage').clientWidth")
    driver.click(driver.element("button[aria-label='Collapse Charts sidebar']"))
    driver.click(driver.element("button[aria-label='Collapse Controls sidebar']"))
    driver.wait(
        lambda: driver.execute(
            f"""
            const workbench=document.querySelector('#workbench');
            return workbench.classList.contains('left-collapsed')
              && workbench.classList.contains('right-collapsed')
              && document.querySelector('#wheel-stage').clientWidth > {stage_width};
            """
        ),
        "completed independent desktop sidebar rail transition",
    )
    collapsed_layout = driver.execute(
        """
        return {
          width: document.querySelector('#wheel-stage').clientWidth,
          outer: document.querySelector('.outer-identity').textContent,
          wheel: Boolean(document.querySelector('#oracle-transit-biwheel')),
          stored: JSON.parse(localStorage.getItem('oracle-studio.layout.v1')),
        };
        """
    )
    if (
        collapsed_layout["width"] <= stage_width
        or collapsed_layout["outer"] != outer_before_layout
        or not collapsed_layout["wheel"]
        or collapsed_layout["stored"]
        != {"schema_version": 1, "left_collapsed": True, "right_collapsed": True}
    ):
        raise RuntimeError(f"desktop sidebar collapse disturbed chart state: {collapsed_layout}")
    driver.click(driver.element("button[aria-label='Expand Charts sidebar']"))
    driver.click(driver.element("button[aria-label='Expand Controls sidebar']"))

    driver.request(
        "POST", driver.path("/window/rect"), {"width": 768, "height": 1024, "x": 0, "y": 0}
    )
    tablet = driver.execute(
        """
        return {
          overflow: document.documentElement.scrollWidth > document.documentElement.clientWidth,
          cards: [...document.querySelectorAll('.chart-identity')].map(card => parseFloat(getComputedStyle(card).fontSize)),
          chartsToggle: getComputedStyle(document.querySelector('.charts-toggle')).display !== 'none',
        };
        """
    )
    if tablet["overflow"] or not tablet["chartsToggle"] or any(size < 14.4 for size in tablet["cards"]):
        raise RuntimeError(f"768x1024 chart layout is not readable and responsive: {tablet}")
    driver.click_text("Charts")
    if len(driver.elements(".left-sidebar.drawer-open")) != 1:
        raise RuntimeError("768x1024 Charts drawer did not open")
    driver.click(driver.element(".left-sidebar .drawer-close"))

    driver.request(
        "POST", driver.path("/window/rect"), {"width": 390, "height": 844, "x": 0, "y": 0}
    )
    mobile = driver.execute(
        """
        const strip=document.querySelector('.wheel-identities').getBoundingClientRect();
        const viewport=document.querySelector('.chart-viewport').getBoundingClientRect();
        return {
          overflow: document.documentElement.scrollWidth > document.documentElement.clientWidth,
          separated: strip.bottom <= viewport.top + 1,
          cards: [...document.querySelectorAll('.chart-identity')].map(card => parseFloat(getComputedStyle(card).fontSize)),
        };
        """
    )
    if mobile["overflow"] or not mobile["separated"] or any(size < 14.4 for size in mobile["cards"]):
        raise RuntimeError(f"390x844 metadata strip overlaps or is unreadable: {mobile}")
    driver.click_text("Controls")
    if len(driver.elements(".right-sidebar.drawer-open .time-column")) != 8:
        raise RuntimeError("390x844 Controls drawer lost one or more time columns")
    driver.click(driver.element(".right-sidebar .drawer-close"))
    driver.request(
        "POST", driver.path("/window/rect"), {"width": 1440, "height": 900, "x": 0, "y": 0}
    )
    print("PASS chart workspace: zoom, persistent desktop rails, readable metadata, and three responsive viewports")

    driver.click_text("Files", "a")
    driver.wait_text("Charts in active workspace")
    driver.wait_text("Scratch workspace · save it as an encrypted vault first")
    scratch_form = "form.save-scratch"
    driver.set_value(driver.control(scratch_form, "Public title"), "Fictional Portable Studio")
    driver.set_value(driver.control(scratch_form, "Password"), "fictional browser password")
    driver.click_text("Save encrypted vault")
    driver.wait_text("Fictional Portable Studio", timeout=90)
    driver.wait_text("ACTIVE", timeout=90)
    driver.wait_text("Ready to save into the currently active unlocked vault.", timeout=120)

    save_as_form = "form.save-as-chart"
    driver.set_value(driver.control(save_as_form, "New chart name"), "fictional TRANSIT")
    driver.click_text("Save as new chart")
    driver.wait_text("a chart with that name already exists; Save As never overwrites")
    driver.set_value(driver.control(save_as_form, "New chart name"), "Fictional Transit Copy")
    driver.click_text("Save as new chart")
    driver.wait_text("Saved new chart “Fictional Transit Copy”.", timeout=120)

    driver.click_text("Workbench", "a")
    if len(driver.elements("//article[.//strong[normalize-space()='Fictional transit']]", "xpath")) != 1:
        raise RuntimeError("save-as overwrote or duplicated the source chart")
    if len(driver.elements("//article[.//strong[normalize-space()='Fictional Transit Copy']]", "xpath")) != 1:
        raise RuntimeError("save-as did not create a distinct chart")
    driver.click(driver.element(".time-column:nth-child(4) button:nth-child(2)"))
    driver.wait(
        lambda: "2026-08-19" in driver.execute(
            "return document.querySelector('.outer-identity').getAttribute('title')"
        ),
        "second unsaved workbench preview",
        timeout=120,
    )
    driver.click_text("Files", "a")
    driver.wait_text("Ready to save into the currently active unlocked vault.", timeout=120)
    driver.execute("window.confirm = () => true")
    driver.click_text("Update existing chart")
    driver.wait_text("Updated existing chart “Fictional transit”.", timeout=120)
    driver.click_text("Workbench", "a")
    transit_card = driver.element(
        "//article[.//strong[normalize-space()='Fictional transit']]", "xpath"
    )
    if "2026-08-19" not in driver.execute("return arguments[0].innerText", [transit_card]):
        raise RuntimeError("update did not preserve and advance the source chart identity")
    print("PASS chart files: route handoff, collision-safe save-as, confirmation, and identity-preserving update succeed")

    driver.click(driver.element("button[aria-label='Collapse Charts sidebar']"))
    driver.click(driver.element("button[aria-label='Collapse Controls sidebar']"))
    driver.click_text("Files", "a")

    driver.execute("location.reload()")
    driver.wait_text("Browser-local studio ready.")
    persisted_layout = driver.execute(
        """
        const workbench=document.querySelector('#workbench');
        return workbench.classList.contains('left-collapsed')
          && workbench.classList.contains('right-collapsed');
        """
    )
    if not persisted_layout:
        raise RuntimeError("desktop sidebar preferences did not survive reload")
    driver.click_text("Files", "a")
    driver.wait_text("Fictional Portable Studio")
    driver.wait_text("LOCKED")
    card = driver.element("//article[.//h2[normalize-space()='Fictional Portable Studio']]", "xpath")
    password = driver.child(card, ".//label[.//span[normalize-space()='Password']]//input", "xpath")
    driver.set_value(password, "fictional browser password")
    driver.click(driver.child(card, ".//button[normalize-space()='Unlock']", "xpath"))
    driver.click_text("Settings", "a")
    driver.wait_text("Fictional Person", timeout=90)
    driver.wait_text("Fictional Focus", timeout=90)
    persisted_aspect_set = driver.execute(
        "return document.querySelector('.aspect-set-settings select').selectedOptions[0].textContent.trim()"
    )
    if persisted_aspect_set != "Fictional Focus":
        raise RuntimeError(f"aspect-set selection did not persist locally: {persisted_aspect_set}")
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
    driver.click_text("Workbench", "a")
    driver.click(driver.element("button[aria-label='Expand Charts sidebar']"))
    driver.click(driver.element("button[aria-label='Expand Controls sidebar']"))
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
    browser_log = driver.request("POST", driver.path("/log"), {"type": "browser"})
    csp_blocks = [
        entry
        for entry in browser_log
        if "Content Security Policy" in entry.get("message", "")
        or "violates the following Content Security Policy" in entry.get("message", "")
    ]
    if csp_blocks:
        raise RuntimeError(f"runtime content was blocked by CSP: {csp_blocks}")
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
    validate_loopback_url(launch_url, "production")
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
                                    "--window-size=1440,900", f"--user-data-dir={profile}",
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
