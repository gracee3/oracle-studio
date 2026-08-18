(() => {
  'use strict'

  const data = JSON.parse(document.getElementById('oracle-timeline').textContent)
  const timeline = data.timeline
  const frames = timeline.frames
  const svg = document.getElementById('oracle-transit-biwheel')
  const scrubber = document.getElementById('scrubber')
  const timestamp = document.getElementById('timestamp')
  const playPause = document.getElementById('play-pause')
  const reverse = document.getElementById('reverse')
  const forward = document.getElementById('forward')
  const rate = document.getElementById('playback-rate')
  const firstMs = Date.parse(frames[0].timestamp)
  const lastMs = Date.parse(frames[frames.length - 1].timestamp)
  const frameTimes = frames.map(frame => Date.parse(frame.timestamp))
  const natalById = new Map(timeline.natal.points.map(point => [point.id, point]))
  const SVG_NS = 'http://www.w3.org/2000/svg'
  const CENTER = 360
  const TRANSIT_RADIUS = 302
  const ASPECT_RADIUS = 142
  const TICK_RADIUS = 289
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
    return [CENTER + radius * Math.cos(radians), CENTER + radius * Math.sin(radians)]
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

  // The largest empty arc is the cut, which keeps a 359°/0° cluster intact.
  // Repeated pair spreading mirrors the selected AstroChart collision behavior
  // while remaining deterministic for every animation sample.
  function resolveCollisions(points) {
    if (points.length < 2) return points.map(point => point.longitude_degrees)
    const minimumGap = 2 * Math.asin(21 / (2 * TRANSIT_RADIUS)) * 180 / Math.PI
    const sorted = points.map((point, index) => ({ index, angle: (point.longitude_degrees + 360) % 360 })).sort((left, right) => left.angle - right.angle || left.index - right.index)
    let cutAfter = 0
    let largestGap = -1
    sorted.forEach((point, index) => {
      const next = sorted[(index + 1) % sorted.length]
      const gap = (next.angle - point.angle + 360) % 360
      if (gap > largestGap) { largestGap = gap; cutAfter = index }
    })
    const unwrapped = []
    for (let offset = 0; offset < sorted.length; offset += 1) {
      const entry = sorted[(cutAfter + 1 + offset) % sorted.length]
      let angle = entry.angle
      while (unwrapped.length && angle < unwrapped[unwrapped.length - 1].angle) angle += 360
      unwrapped.push({ ...entry, angle })
    }
    const displayed = unwrapped.map(entry => entry.angle)
    for (let pass = 0; pass < displayed.length; pass += 1) {
      for (let index = 1; index < displayed.length; index += 1) {
        const gap = displayed[index] - displayed[index - 1]
        if (gap < minimumGap) {
          const adjustment = (minimumGap - gap) / 2
          displayed[index - 1] -= adjustment
          displayed[index] += adjustment
        }
      }
    }
    const result = new Array(points.length)
    unwrapped.forEach((entry, index) => { result[entry.index] = (displayed[index] + 360) % 360 })
    return result
  }

  function setLine(line, start, end) {
    line.setAttribute('x1', start[0].toFixed(3))
    line.setAttribute('y1', start[1].toFixed(3))
    line.setAttribute('x2', end[0].toFixed(3))
    line.setAttribute('y2', end[1].toFixed(3))
  }

  function updateTransit(points) {
    const displayed = resolveCollisions(points)
    points.forEach((point, index) => {
      const group = document.getElementById(`transit-point-${slug(point.id)}`)
      const actual = visualLongitude(point.longitude_degrees)
      const visual = visualLongitude(displayed[index])
      const actualAtGlyph = polar(actual, TRANSIT_RADIUS)
      const glyph = polar(visual, TRANSIT_RADIUS)
      setLine(group.querySelector('[data-role="leader"]'), actualAtGlyph, glyph)
      setLine(group.querySelector('[data-role="tick"]'), polar(actual, TICK_RADIUS - 4), polar(actual, TICK_RADIUS + 4))
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
      const line = document.createElementNS(SVG_NS, 'line')
      line.setAttribute('id', aspect.id)
      line.setAttribute('class', `aspect aspect--${slug(aspect.kind)}`)
      line.dataset.natalId = aspect.natal_point_id
      line.dataset.transitId = aspect.transit_point_id
      line.dataset.kind = slug(aspect.kind)
      setLine(line, polar(visualLongitude(natal.longitude_degrees), ASPECT_RADIUS), polar(visualLongitude(transit.longitude_degrees), ASPECT_RADIUS))
      const title = document.createElementNS(SVG_NS, 'title')
      title.textContent = `${aspect.natal_point_id} ${aspect.kind} ${aspect.transit_point_id} (orb ${aspect.orb_degrees.toFixed(6)}°, phase ${aspect.phase || 'not supplied'})`
      line.appendChild(title)
      layer.appendChild(line)
    })
  }

  function render(milliseconds) {
    currentMs = Math.max(firstMs, Math.min(lastMs, milliseconds))
    const scene = sample(currentMs)
    updateTransit(scene.points)
    updateAspects(scene.aspects, scene.points)
    scrubber.value = String(Math.round(currentMs))
    timestamp.value = scene.timestamp
    timestamp.textContent = scene.timestamp
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
  document.getElementById('toggle-natal').addEventListener('change', event => document.getElementById('natal-layer').classList.toggle('is-hidden', !event.target.checked))
  document.getElementById('toggle-transit').addEventListener('change', event => document.getElementById('transit-layer').classList.toggle('is-hidden', !event.target.checked))
  document.getElementById('toggle-aspects').addEventListener('change', event => document.getElementById('aspect-layer').classList.toggle('is-hidden', !event.target.checked))

  render(firstMs)
})()
