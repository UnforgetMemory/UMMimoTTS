import { render, screen } from '@testing-library/react'
import { ProgressBar } from './ProgressBar'

function renderProgressBar(value: number, className = '') {
  return render(<ProgressBar value={value} className={className} />)
}

/** Read the width style of the progress bar's inner div. */
function getInnerWidth(container: HTMLElement): string {
  const outer = container.querySelector('.h-1\\.5')
  const inner = outer?.querySelector('[style*="width"]')
  return inner?.getAttribute('style') ?? ''
}

describe('ProgressBar', () => {
  it('value=0 显示 0% 宽度', () => {
    const { container } = renderProgressBar(0)
    expect(getInnerWidth(container)).toContain('0%')
  })

  it('value=1 显示 100% 宽度', () => {
    const { container } = renderProgressBar(1)
    expect(getInnerWidth(container)).toContain('100%')
  })

  it('value=0.5 显示 50% 宽度', () => {
    const { container } = renderProgressBar(0.5)
    expect(getInnerWidth(container)).toContain('50%')
  })

  it('value=0.333 显示 33% 宽度（四舍五入）', () => {
    const { container } = renderProgressBar(0.333)
    expect(getInnerWidth(container)).toContain('33%')
  })

  it('value=0.6667 显示 67% 宽度（四舍五入）', () => {
    const { container } = renderProgressBar(0.6667)
    expect(getInnerWidth(container)).toContain('67%')
  })

  it('value=-1 钳制到 0%', () => {
    const { container } = renderProgressBar(-1)
    expect(getInnerWidth(container)).toContain('0%')
  })

  it('value=-0.5 钳制到 0%', () => {
    const { container } = renderProgressBar(-0.5)
    expect(getInnerWidth(container)).toContain('0%')
  })

  it('value=2 钳制到 100%', () => {
    const { container } = renderProgressBar(2)
    expect(getInnerWidth(container)).toContain('100%')
  })

  it('value=1.5 钳制到 100%', () => {
    const { container } = renderProgressBar(1.5)
    expect(getInnerWidth(container)).toContain('100%')
  })

  it('value=0.995 四舍五入到 100%', () => {
    const { container } = renderProgressBar(0.995)
    expect(getInnerWidth(container)).toContain('100%')
  })

  it('value=0.994 四舍五入到 99%', () => {
    const { container } = renderProgressBar(0.994)
    expect(getInnerWidth(container)).toContain('99%')
  })

  it('value=0.004 四舍五入到 0%', () => {
    const { container } = renderProgressBar(0.004)
    expect(getInnerWidth(container)).toContain('0%')
  })

  it('value=0.005 四舍五入到 1%', () => {
    const { container } = renderProgressBar(0.005)
    expect(getInnerWidth(container)).toContain('1%')
  })

  it('className 传递到容器', () => {
    const { container } = renderProgressBar(0.5, 'my-extra-class')
    const outer = container.querySelector('.h-1\\.5')
    expect(outer).toHaveClass('my-extra-class')
  })

  it('渲染两个嵌套 div', () => {
    const { container } = renderProgressBar(0.5)
    const outer = container.querySelector('.h-1\\.5')
    const inner = outer?.querySelector('div')
    expect(outer).toBeInTheDocument()
    expect(inner).toBeInTheDocument()
  })

  it('内层 div 包含过渡动画类', () => {
    const { container } = renderProgressBar(0.5)
    const outer = container.querySelector('.h-1\\.5')
    const inner = outer?.querySelector('div')
    expect(inner).toHaveClass('transition-[width]')
  })
})
