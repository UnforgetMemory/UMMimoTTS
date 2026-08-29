import { render, screen, fireEvent } from '@testing-library/react'
import { useThemeStore } from '@/stores/theme'
import { ThemeToggle } from './ThemeToggle'

describe('ThemeToggle', () => {
  beforeEach(() => {
    useThemeStore.setState({ theme: 'dark' })
  })

  it('渲染按钮', () => {
    render(<ThemeToggle />)
    expect(screen.getByRole('button')).toBeInTheDocument()
  })

  it('按钮 type 为 button', () => {
    render(<ThemeToggle />)
    expect(screen.getByRole('button')).toHaveAttribute('type', 'button')
  })

  it('深色模式：aria-label 为"切换到浅色主题"', () => {
    useThemeStore.setState({ theme: 'dark' })
    render(<ThemeToggle />)
    expect(screen.getByRole('button')).toHaveAttribute('aria-label', '切换到浅色主题')
  })

  it('浅色模式：aria-label 为"切换到深色主题"', () => {
    useThemeStore.setState({ theme: 'light' })
    render(<ThemeToggle />)
    expect(screen.getByRole('button')).toHaveAttribute('aria-label', '切换到深色主题')
  })

  it('深色模式：title 为"切换到浅色主题"', () => {
    useThemeStore.setState({ theme: 'dark' })
    render(<ThemeToggle />)
    expect(screen.getByRole('button')).toHaveAttribute('title', '切换到浅色主题')
  })

  it('浅色模式：title 为"切换到深色主题"', () => {
    useThemeStore.setState({ theme: 'light' })
    render(<ThemeToggle />)
    expect(screen.getByRole('button')).toHaveAttribute('title', '切换到深色主题')
  })

  it('深色模式渲染 SunIcon（svg 含 circle 和 path）', () => {
    useThemeStore.setState({ theme: 'dark' })
    render(<ThemeToggle />)
    const svg = document.querySelector('svg')
    expect(svg).toBeInTheDocument()
    expect(svg).toHaveAttribute('aria-hidden', 'true')
    expect(svg?.querySelector('circle')).toBeInTheDocument()
    expect(svg?.querySelector('path')).toBeInTheDocument()
  })

  it('浅色模式渲染 MoonIcon（svg 含 path）', () => {
    useThemeStore.setState({ theme: 'light' })
    render(<ThemeToggle />)
    const svg = document.querySelector('svg')
    expect(svg).toBeInTheDocument()
    // MoonIcon has paths only — no circle
    expect(svg?.querySelector('circle')).toBeNull()
    expect(svg?.querySelector('path')).toBeInTheDocument()
  })

  it('点击切换主题', () => {
    useThemeStore.setState({ theme: 'dark' })
    render(<ThemeToggle />)
    const button = screen.getByRole('button')
    fireEvent.click(button)
    // theme should flip dark → light on click
    expect(useThemeStore.getState().theme).toBe('light')
  })

  it('浅色模式点击切换为深色', () => {
    useThemeStore.setState({ theme: 'light' })
    render(<ThemeToggle />)
    fireEvent.click(screen.getByRole('button'))
    expect(useThemeStore.getState().theme).toBe('dark')
  })

  it('多次点击交替切换', () => {
    useThemeStore.setState({ theme: 'dark' })
    render(<ThemeToggle />)
    const button = screen.getByRole('button')
    fireEvent.click(button)
    expect(useThemeStore.getState().theme).toBe('light')
    fireEvent.click(button)
    expect(useThemeStore.getState().theme).toBe('dark')
    fireEvent.click(button)
    expect(useThemeStore.getState().theme).toBe('light')
  })

  it('深色模式下 SunIcon circle 属性正确', () => {
    useThemeStore.setState({ theme: 'dark' })
    render(<ThemeToggle />)
    const svg = document.querySelector('svg')
    const circle = svg?.querySelector('circle')
    expect(circle).toHaveAttribute('cx', '12')
    expect(circle).toHaveAttribute('cy', '12')
    expect(circle).toHaveAttribute('r', '4')
  })
})
