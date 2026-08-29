import { render } from '@testing-library/react'
import { PageLoader } from './PageLoader'

/** Extract the Spinner span from a render result. */
function getSpinner(container: HTMLElement): HTMLElement {
  return container.querySelector('span') as HTMLElement
}

describe('PageLoader', () => {
  it('渲染 Spinner 组件', () => {
    const { container } = render(<PageLoader />)
    const spinner = getSpinner(container)
    expect(spinner).toBeInTheDocument()
  })

  it('Spinner 包含 animate-spin 类', () => {
    const { container } = render(<PageLoader />)
    const spinner = getSpinner(container)
    expect(spinner).toHaveClass('animate-spin')
  })

  it('Spinner 包含 border 类', () => {
    const { container } = render(<PageLoader />)
    const spinner = getSpinner(container)
    expect(spinner).toHaveClass('border-2')
  })

  it('Spinner 包含 rounded-full 类', () => {
    const { container } = render(<PageLoader />)
    const spinner = getSpinner(container)
    expect(spinner).toHaveClass('rounded-full')
  })

  it('容器使用 flex 居中布局', () => {
    const { container } = render(<PageLoader />)
    const wrapper = container.firstChild as HTMLElement
    expect(wrapper).toHaveClass('flex')
    expect(wrapper).toHaveClass('items-center')
    expect(wrapper).toHaveClass('justify-center')
  })

  it('容器包含最小高度', () => {
    const { container } = render(<PageLoader />)
    const wrapper = container.firstChild as HTMLElement
    expect(wrapper).toHaveClass('min-h-[40vh]')
  })

  it('Spinner 带尺寸类 h-6 w-6', () => {
    const { container } = render(<PageLoader />)
    const spinner = getSpinner(container)
    expect(spinner).toHaveClass('h-6', 'w-6')
  })
})
