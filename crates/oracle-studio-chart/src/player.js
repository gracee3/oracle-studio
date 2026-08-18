(() => {
  'use strict'

  const data = JSON.parse(document.getElementById('oracle-timeline').textContent)
  const timeline = data.timeline
  const frames = timeline.frames
  const svg = document.getElementById('oracle-transit-biwheel')
  const scrubber = document.getElementById('scrubber')
  const timestamp = document.getElementById('timestamp')
  const transitChartDatetime = document.getElementById('transit-chart-datetime')
  const playPause = document.getElementById('play-pause')
  const reverse = document.getElementById('reverse')
  const forward = document.getElementById('forward')
  const rate = document.getElementById('playback-rate')
  const firstMs = Date.parse(frames[0].timestamp)
  const lastMs = Date.parse(frames[frames.length - 1].timestamp)
  const frameTimes = frames.map(frame => Date.parse(frame.timestamp))
  const natalById = new Map(timeline.natal.points.map(point => [point.id, point]))
  const SVG_NS = 'http://www.w3.org/2000/svg'
  const PATH_GLYPHS = new Set(['Sun', 'Moon', 'Mercury', 'Venus', 'Mars', 'Jupiter', 'Saturn', 'Uranus', 'Neptune', 'Pluto', 'Chiron', 'MeanNode', 'TrueNode', 'MeanSouthNode', 'TrueSouthNode'])
  const geometry = {
    center: Number(svg.dataset.center),
    aspectRadius: Number(svg.dataset.aspectRadius),
    transit: {
      innerRadius: Number(svg.dataset.transitInnerRadius),
      positionRadius: Number(svg.dataset.transitPositionRadius),
      glyphRadius: Number(svg.dataset.transitGlyphRadius)
    },
    labelPadding: Number(svg.dataset.labelPadding)
  }
  const MAX_INTERPOLATION_MS = 24 * 60 * 60 * 1000
  let currentMs = firstMs
  let direction = 1
  let playing = false
  let previousAnimationTime = null

  scrubber.min = String(firstMs)
  scrubber.max = String(lastMs)
  scrubber.value = String(firstMs)

  function slug(value) {
    const result = value.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '')
    return result || 'unknown'
  }

  function visualLongitude(longitude) {
    if (data.orientation === 'ascendant-left') {
      return (longitude - timeline.natal.ascendant_degrees + 630) % 360
    }
    return (longitude + 360) % 360
  }

  function polar(longitude, radius) {
    const radians = (longitude - 90) * Math.PI / 180
    return [geometry.center + radius * Math.cos(radians), geometry.center + radius * Math.sin(radians)]
  }

  function directionFor(speed) {
    return speed > 1e-12 ? 1 : speed < -1e-12 ? -1 : 0
  }

  function directedDelta(left, right) {
    const leftDirection = directionFor(left.longitude_speed_degrees_per_day)
    const rightDirection = directionFor(right.longitude_speed_degrees_per_day)
    const motion = leftDirection === rightDirection ? leftDirection : leftDirection === 0 ? rightDirection : rightDirection === 0 ? leftDirection : 0
    if (motion > 0) return (right.longitude_degrees - left.longitude_degrees + 360) % 360
    if (motion < 0) return -((left.longitude_degrees - right.longitude_degrees + 360) % 360)
    const forwardDistance = (right.longitude_degrees - left.longitude_degrees + 360) % 360
    return forwardDistance > 180 ? forwardDistance - 360 : forwardDistance
  }

  function interpolatePoint(left, right, ratio) {
    const speed = left.longitude_speed_degrees_per_day + (right.longitude_speed_degrees_per_day - left.longitude_speed_degrees_per_day) * ratio
    return {
      ...left,
      longitude_degrees: (left.longitude_degrees + directedDelta(left, right) * ratio + 360) % 360,
      longitude_speed_degrees_per_day: speed,
      retrograde: speed < -1e-12
    }
  }

  function sample(milliseconds) {
    if (milliseconds <= firstMs) return { timestamp: frames[0].timestamp, points: frames[0].points, aspects: frames[0].aspects }
    if (milliseconds >= lastMs) return { timestamp: frames[frames.length - 1].timestamp, points: frames[frames.length - 1].points, aspects: frames[frames.length - 1].aspects }
    for (let index = 0; index < frames.length - 1; index += 1) {
      const leftTime = frameTimes[index]
      const rightTime = frameTimes[index + 1]
      if (milliseconds === rightTime) return { timestamp: frames[index + 1].timestamp, points: frames[index + 1].points, aspects: frames[index + 1].aspects }
      if (milliseconds > leftTime && milliseconds < rightTime) {
        if (rightTime - leftTime > MAX_INTERPOLATION_MS) {
          return { timestamp: new Date(milliseconds).toISOString(), points: frames[index].points, aspects: frames[index].aspects }
        }
        const ratio = (milliseconds - leftTime) / (rightTime - leftTime)
        return {
          timestamp: new Date(milliseconds).toISOString(),
          points: frames[index].points.map((point, pointIndex) => interpolatePoint(point, frames[index + 1].points[pointIndex], ratio)),
          aspects: frames[index].aspects
        }
      }
    }
    return { timestamp: frames[frames.length - 1].timestamp, points: frames[frames.length - 1].points, aspects: frames[frames.length - 1].aspects }
  }

  function tokenWidths(point, precision) {
    const position = precision === 'arcminute' ? 40 : 24
    let glyph
    if (PATH_GLYPHS.has(point.id)) glyph = point.retrograde ? 28 : 18
    else glyph = point.retrograde ? 34 : 24
    return [position, glyph]
  }

  function requiredGap(left, right, lane, precision) {
    const leftWidths = tokenWidths(left, precision)
    const rightWidths = tokenWidths(right, precision)
    const radii = [lane.positionRadius, lane.glyphRadius]
    return Math.max(...radii.map((radius, index) => {
      const distance = (leftWidths[index] + rightWidths[index]) / 2 + geometry.labelPadding
      return 2 * Math.asin(Math.max(0, Math.min(1, distance / (2 * radius)))) * 180 / Math.PI
    }))
  }

  // Match Rust's wrap-aware adaptive layout: isolated labels keep arcminutes;
  // collision clusters switch to degrees before centered constrained spreading.
  function layoutLabels(points, lane) {
    if (points.length === 0) return []
    if (points.length === 1) return [{ displayLongitude: ((points[0].longitude_degrees % 360) + 360) % 360, precision: 'arcminute' }]
    const sorted = points.map((point, index) => ({ index, angle: (point.longitude_degrees + 360) % 360 })).sort((left, right) => left.angle - right.angle || left.index - right.index)
    let cutAfter = 0
    let largestGap = -1
    sorted.forEach((point, index) => {
      const next = sorted[(index + 1) % sorted.length]
      const gap = (next.angle - point.angle + 360) % 360
      if (gap >= largestGap) { largestGap = gap; cutAfter = index }
    })
    const unwrapped = []
    for (let offset = 0; offset < sorted.length; offset += 1) {
      const entry = sorted[(cutAfter + 1 + offset) % sorted.length]
      let angle = entry.angle
      while (unwrapped.length && angle < unwrapped[unwrapped.length - 1].angle) angle += 360
      unwrapped.push({ ...entry, angle })
    }
    const result = points.map(point => ({ displayLongitude: ((point.longitude_degrees % 360) + 360) % 360, precision: 'arcminute' }))
    let clusterStart = 0
    for (let index = 1; index <= unwrapped.length; index += 1) {
      const continues = index < unwrapped.length && unwrapped[index].angle - unwrapped[index - 1].angle < requiredGap(points[unwrapped[index - 1].index], points[unwrapped[index].index], lane, 'arcminute')
      if (continues) continue
      if (index - clusterStart > 1) {
        const cluster = unwrapped.slice(clusterStart, index)
        const offsets = new Array(cluster.length).fill(0)
        for (let offset = 1; offset < cluster.length; offset += 1) {
          offsets[offset] = offsets[offset - 1] + requiredGap(points[cluster[offset - 1].index], points[cluster[offset].index], lane, 'degree')
        }
        const mean = cluster.reduce((sum, entry, offset) => sum + entry.angle - offsets[offset], 0) / cluster.length
        cluster.forEach((entry, offset) => {
          result[entry.index] = { displayLongitude: ((mean + offsets[offset]) % 360 + 360) % 360, precision: 'degree' }
        })
      }
      clusterStart = index
    }
    return result
  }

  function roundPosition(longitude, precision) {
    if (precision === 'arcminute') {
      const total = ((Math.round((((longitude % 360) + 360) % 360) * 60) % 21600) + 21600) % 21600
      const withinSign = total % 1800
      return { signIndex: Math.floor(total / 1800), degrees: Math.floor(withinSign / 60), minutes: withinSign % 60 }
    }
    const total = ((Math.round(((longitude % 360) + 360) % 360) % 360) + 360) % 360
    return { signIndex: Math.floor(total / 30), degrees: total % 30, minutes: null }
  }

  function pad2(value) {
    return String(value).padStart(2, '0')
  }

  function setLine(line, start, end) {
    line.setAttribute('x1', start[0].toFixed(3))
    line.setAttribute('y1', start[1].toFixed(3))
    line.setAttribute('x2', end[0].toFixed(3))
    line.setAttribute('y2', end[1].toFixed(3))
  }

  function updateTransit(points) {
    const layouts = layoutLabels(points, geometry.transit)
    points.forEach((point, index) => {
      const group = document.getElementById(`transit-point-${slug(point.id)}`)
      const actual = visualLongitude(point.longitude_degrees)
      const layout = layouts[index]
      const visual = visualLongitude(layout.displayLongitude)
      const position = polar(visual, geometry.transit.positionRadius)
      const glyph = polar(visual, geometry.transit.glyphRadius)
      setLine(group.querySelector('[data-role="leader"]'), polar(actual, geometry.transit.innerRadius + 4), polar(visual, geometry.transit.positionRadius - 9))
      setLine(group.querySelector('[data-role="tick"]'), polar(actual, geometry.transit.innerRadius - 4), polar(actual, geometry.transit.innerRadius + 4))
      const rounded = roundPosition(point.longitude_degrees, layout.precision)
      const positionElement = group.querySelector('[data-role="position"]')
      positionElement.setAttribute('x', position[0].toFixed(3))
      positionElement.setAttribute('y', position[1].toFixed(3))
      positionElement.dataset.signIndex = String(rounded.signIndex)
      positionElement.textContent = rounded.minutes === null ? `${pad2(rounded.degrees)}°` : `${pad2(rounded.degrees)}°${pad2(rounded.minutes)}′`
      const glyphElement = group.querySelector('[data-role="glyph"]')
      if (glyphElement.tagName.toLowerCase() === 'g') glyphElement.setAttribute('transform', `translate(${glyph[0].toFixed(3)} ${glyph[1].toFixed(3)})`)
      else { glyphElement.setAttribute('x', glyph[0].toFixed(3)); glyphElement.setAttribute('y', glyph[1].toFixed(3)) }
      let marker = group.querySelector('[data-role="motion"]')
      if (point.retrograde && !marker) {
        marker = document.createElementNS(SVG_NS, 'text')
        marker.setAttribute('data-role', 'motion')
        marker.setAttribute('class', 'motion-marker')
        marker.textContent = '℞'
        group.appendChild(marker)
      }
      if (marker) {
        marker.setAttribute('x', (glyph[0] + 9).toFixed(3))
        marker.setAttribute('y', (glyph[1] + 10).toFixed(3))
        marker.classList.toggle('is-hidden', !point.retrograde)
      }
      group.dataset.longitude = point.longitude_degrees.toFixed(12)
      group.dataset.displayLongitude = layout.displayLongitude.toFixed(12)
      group.dataset.precision = layout.precision
      group.querySelector('title').textContent = `Transit ${point.id} at ${point.longitude_degrees.toFixed(6)}°, speed ${point.longitude_speed_degrees_per_day.toFixed(6)}° per day${point.retrograde ? ', retrograde' : ''}`
    })
  }

  function updateAspects(aspects, transitPoints) {
    const transitById = new Map(transitPoints.map(point => [point.id, point]))
    const layer = document.getElementById('aspect-layer')
    layer.replaceChildren()
    aspects.forEach(aspect => {
      const natal = natalById.get(aspect.natal_point_id)
      const transit = transitById.get(aspect.transit_point_id)
      if (!natal || !transit) return
      const group = document.createElementNS(SVG_NS, 'g')
      group.setAttribute('id', aspect.id)
      group.setAttribute('class', `aspect aspect--${slug(aspect.kind)}`)
      group.dataset.natalId = aspect.natal_point_id
      group.dataset.transitId = aspect.transit_point_id
      group.dataset.kind = slug(aspect.kind)
      const title = document.createElementNS(SVG_NS, 'title')
      title.textContent = `${aspect.natal_point_id} ${aspect.kind} ${aspect.transit_point_id} (orb ${aspect.orb_degrees.toFixed(6)}°, phase ${aspect.phase || 'not supplied'})`
      group.appendChild(title)
      const start = polar(visualLongitude(natal.longitude_degrees), geometry.aspectRadius)
      const end = polar(visualLongitude(transit.longitude_degrees), geometry.aspectRadius)
      const line = document.createElementNS(SVG_NS, 'line')
      line.setAttribute('id', `${aspect.id}--line`)
      line.setAttribute('data-role', 'aspect-line')
      setLine(line, start, end)
      group.appendChild(line)
      const glyph = document.createElementNS(SVG_NS, 'text')
      glyph.setAttribute('id', `${aspect.id}--glyph`)
      glyph.setAttribute('data-role', 'aspect-glyph')
      glyph.setAttribute('class', 'aspect-glyph')
      glyph.setAttribute('x', ((start[0] + end[0]) / 2).toFixed(3))
      glyph.setAttribute('y', ((start[1] + end[1]) / 2).toFixed(3))
      glyph.textContent = aspectGlyph(aspect.kind)
      group.appendChild(glyph)
      layer.appendChild(group)
    })
  }

  function aspectGlyph(kind) {
    return { Conjunction: '☌', Sextile: '⚹', Square: '□', Trine: '△', Opposition: '☍' }[kind] || '·'
  }

  function offsetText(offsetSeconds) {
    const sign = offsetSeconds < 0 ? '-' : '+'
    const absolute = Math.abs(offsetSeconds)
    return `${sign}${pad2(Math.floor(absolute / 3600))}:${pad2(Math.floor((absolute % 3600) / 60))}`
  }

  function formatChartDatetime(milliseconds, offsetSeconds) {
    const local = new Date(milliseconds + offsetSeconds * 1000)
    const days = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat']
    const months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']
    return `${days[local.getUTCDay()]}, ${months[local.getUTCMonth()]} ${pad2(local.getUTCDate())}, ${local.getUTCFullYear()} · ${pad2(local.getUTCHours())}:${pad2(local.getUTCMinutes())} ${offsetText(offsetSeconds)}`
  }

  function machineChartDatetime(milliseconds, offsetSeconds) {
    const local = new Date(milliseconds + offsetSeconds * 1000)
    return `${local.toISOString().slice(0, -1)}${offsetText(offsetSeconds)}`
  }

  function render(milliseconds) {
    currentMs = Math.max(firstMs, Math.min(lastMs, milliseconds))
    const scene = sample(currentMs)
    updateTransit(scene.points)
    updateAspects(scene.aspects, scene.points)
    scrubber.value = String(Math.round(currentMs))
    timestamp.value = scene.timestamp
    timestamp.textContent = scene.timestamp
    const sceneMs = Date.parse(scene.timestamp)
    transitChartDatetime.dateTime = machineChartDatetime(sceneMs, data.transit_offset_seconds)
    transitChartDatetime.textContent = formatChartDatetime(sceneMs, data.transit_offset_seconds)
  }

  function setDirection(nextDirection) {
    direction = nextDirection
    reverse.setAttribute('aria-pressed', String(direction < 0))
    forward.setAttribute('aria-pressed', String(direction > 0))
  }

  function setPlaying(nextPlaying) {
    playing = nextPlaying
    playPause.setAttribute('aria-pressed', String(playing))
    playPause.textContent = playing ? 'Pause' : 'Play'
    previousAnimationTime = null
    if (playing) requestAnimationFrame(animate)
  }

  function animate(animationTime) {
    if (!playing) return
    if (previousAnimationTime !== null) {
      const elapsedSeconds = (animationTime - previousAnimationTime) / 1000
      const chartSecondsPerSecond = Number(rate.value)
      render(currentMs + elapsedSeconds * chartSecondsPerSecond * 1000 * direction)
      if (currentMs === firstMs || currentMs === lastMs) setPlaying(false)
    }
    previousAnimationTime = animationTime
    if (playing) requestAnimationFrame(animate)
  }

  function stepExact(step) {
    const candidate = step > 0 ? frameTimes.find(value => value > currentMs) : [...frameTimes].reverse().find(value => value < currentMs)
    render(candidate === undefined ? (step > 0 ? lastMs : firstMs) : candidate)
  }

  scrubber.addEventListener('input', () => render(Number(scrubber.value)))
  playPause.addEventListener('click', () => setPlaying(!playing))
  reverse.addEventListener('click', () => setDirection(-1))
  forward.addEventListener('click', () => setDirection(1))
  document.getElementById('previous-frame').addEventListener('click', () => stepExact(-1))
  document.getElementById('next-frame').addEventListener('click', () => stepExact(1))
  document.getElementById('toggle-natal').addEventListener('change', event => {
    document.getElementById('natal-structure-layer').classList.toggle('is-hidden', !event.target.checked)
    document.getElementById('natal-layer').classList.toggle('is-hidden', !event.target.checked)
  })
  document.getElementById('toggle-transit').addEventListener('change', event => document.getElementById('transit-layer').classList.toggle('is-hidden', !event.target.checked))
  document.getElementById('toggle-aspects').addEventListener('change', event => document.getElementById('aspect-layer').classList.toggle('is-hidden', !event.target.checked))

  render(firstMs)
})()
