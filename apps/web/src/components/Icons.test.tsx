import type { ComponentType, SVGProps } from 'react'
import { render } from '@testing-library/react'
import {
  SunIcon,
  MoonIcon,
  PlayIcon,
  PauseIcon,
  UploadIcon,
  DownloadIcon,
  SearchIcon,
  RefreshIcon,
  TrashIcon,
  XIcon,
  CheckIcon,
  ChevronLeftIcon,
  SettingsIcon,
  ZapIcon,
  ListIcon,
  FileIcon,
} from './Icons'

/** Extract the svg element from a render result. */
function getSvg(container: HTMLElement): SVGElement {
  return container.querySelector('svg') as SVGElement
}

describe('Icons', () => {
  const iconComponents = [
    ['SunIcon', SunIcon],
    ['MoonIcon', MoonIcon],
    ['PlayIcon', PlayIcon],
    ['PauseIcon', PauseIcon],
    ['UploadIcon', UploadIcon],
    ['DownloadIcon', DownloadIcon],
    ['SearchIcon', SearchIcon],
    ['RefreshIcon', RefreshIcon],
    ['TrashIcon', TrashIcon],
    ['XIcon', XIcon],
    ['CheckIcon', CheckIcon],
    ['ChevronLeftIcon', ChevronLeftIcon],
    ['SettingsIcon', SettingsIcon],
    ['ZapIcon', ZapIcon],
    ['ListIcon', ListIcon],
    ['FileIcon', FileIcon],
  ] as const

  it.each(iconComponents)('%s 渲染为 svg 元素且 aria-hidden', (_label: string, Icon: ComponentType<SVGProps<SVGSVGElement>>) => {
    const { container } = render(<Icon />)
    const svg = getSvg(container)
    expect(svg).toBeInTheDocument()
    expect(svg).toHaveAttribute('aria-hidden', 'true')
    expect(svg).toHaveAttribute('viewBox', '0 0 24 24')
  })

  it.each(iconComponents)('%s 接受 className 并传递', (_label: string, Icon: ComponentType<SVGProps<SVGSVGElement>>) => {
    const { container } = render(<Icon className="h-5 w-5 text-red-500" />)
    const svg = getSvg(container)
    expect(svg).toHaveClass('h-5', 'w-5', 'text-red-500')
  })

  it.each(iconComponents)('%s 接受其他 props 并传递', (_label: string, Icon: ComponentType<SVGProps<SVGSVGElement>>) => {
    const { container } = render(<Icon data-testid="my-icon" />)
    expect(container.querySelector('[data-testid="my-icon"]')).toBeInTheDocument()
  })

  it.each(iconComponents)('%s 默认 fill=none stroke=currentColor', (_label: string, Icon: ComponentType<SVGProps<SVGSVGElement>>) => {
    const { container } = render(<Icon />)
    const svg = getSvg(container)
    expect(svg).toHaveAttribute('fill', 'none')
    expect(svg).toHaveAttribute('stroke', 'currentColor')
    expect(svg).toHaveAttribute('stroke-width', '2')
    expect(svg).toHaveAttribute('stroke-linecap', 'round')
    expect(svg).toHaveAttribute('stroke-linejoin', 'round')
  })

  describe('特定图标子元素', () => {
    it('SunIcon 包含 circle 和 path', () => {
      const { container } = render(<SunIcon />)
      const svg = getSvg(container)
      expect(svg.querySelector('circle')).toBeInTheDocument()
      expect(svg.querySelector('path')).toBeInTheDocument()
    })

    it('MoonIcon 包含 path', () => {
      const { container } = render(<MoonIcon />)
      const svg = getSvg(container)
      expect(svg.querySelector('path')).toBeInTheDocument()
      expect(svg.querySelector('circle')).toBeNull()
    })

    it('PlayIcon 包含 polygon（实心填充）', () => {
      const { container } = render(<PlayIcon />)
      const svg = getSvg(container)
      const polygon = svg.querySelector('polygon')
      expect(polygon).toBeInTheDocument()
      expect(polygon).toHaveAttribute('fill', 'currentColor')
      expect(polygon).toHaveAttribute('stroke', 'none')
    })

    it('PauseIcon 包含两个 rect', () => {
      const { container } = render(<PauseIcon />)
      const svg = getSvg(container)
      const rects = svg.querySelectorAll('rect')
      expect(rects).toHaveLength(2)
    })

    it('SearchIcon 包含 circle 和 line', () => {
      const { container } = render(<SearchIcon />)
      const svg = getSvg(container)
      expect(svg.querySelector('circle')).toBeInTheDocument()
      expect(svg.querySelector('line')).toBeInTheDocument()
    })

    it('CheckIcon 包含 polyline', () => {
      const { container } = render(<CheckIcon />)
      const svg = getSvg(container)
      expect(svg.querySelector('polyline')).toBeInTheDocument()
    })

    it('TrashIcon 包含 polyline 和 path', () => {
      const { container } = render(<TrashIcon />)
      const svg = getSvg(container)
      expect(svg.querySelector('polyline')).toBeInTheDocument()
      expect(svg.querySelector('path')).toBeInTheDocument()
    })

    it('FileIcon 包含 path 和 polyline', () => {
      const { container } = render(<FileIcon />)
      const svg = getSvg(container)
      expect(svg.querySelector('path')).toBeInTheDocument()
      expect(svg.querySelector('polyline')).toBeInTheDocument()
    })

    it('UploadIcon 包含 path、polyline、line', () => {
      const { container } = render(<UploadIcon />)
      const svg = getSvg(container)
      expect(svg.querySelector('path')).toBeInTheDocument()
      expect(svg.querySelector('polyline')).toBeInTheDocument()
      expect(svg.querySelector('line')).toBeInTheDocument()
    })

    it('ZapIcon 包含 polygon', () => {
      const { container } = render(<ZapIcon />)
      const svg = getSvg(container)
      expect(svg.querySelector('polygon')).toBeInTheDocument()
    })

    it('ListIcon 包含多个 line', () => {
      const { container } = render(<ListIcon />)
      const svg = getSvg(container)
      const lines = svg.querySelectorAll('line')
      expect(lines.length).toBeGreaterThanOrEqual(6)
    })
  })

  it('不传入 props 时正常渲染', () => {
    const { container } = render(<SunIcon />)
    expect(container.firstChild).toBeInTheDocument()
  })

  it('接受 aria-label 等附加属性', () => {
    const { container } = render(<SunIcon aria-label="太阳" role="img" />)
    const svg = getSvg(container)
    expect(svg).toHaveAttribute('aria-label', '太阳')
    expect(svg).toHaveAttribute('role', 'img')
  })
})
