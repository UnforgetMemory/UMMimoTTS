import { render, screen, fireEvent } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Spinner, ErrorNotice, EmptyState, Label, Button, TextInput, TextArea, Select, Card } from './ui'

/** Extract the Spinner span from a render result. */
function getSpinner(container: HTMLElement): HTMLElement {
  return container.querySelector('span') as HTMLElement
}

describe('Spinner', () => {
  it('渲染 span 元素', () => {
    const { container } = render(<Spinner />)
    const spinner = getSpinner(container)
    expect(spinner.tagName).toBe('SPAN')
  })

  it('aria-hidden="true"', () => {
    const { container } = render(<Spinner />)
    expect(getSpinner(container)).toHaveAttribute('aria-hidden', 'true')
  })

  it('包含 animate-spin 类', () => {
    const { container } = render(<Spinner />)
    expect(getSpinner(container)).toHaveClass('animate-spin')
  })

  it('className 传递', () => {
    const { container } = render(<Spinner className="h-8 w-8 text-blue-500" />)
    expect(getSpinner(container)).toHaveClass('h-8', 'w-8', 'text-blue-500')
  })

  it('默认尺寸 h-4 w-4', () => {
    const { container } = render(<Spinner />)
    expect(getSpinner(container)).toHaveClass('h-4', 'w-4')
  })
})

describe('ErrorNotice', () => {
  it('message=null 不渲染 alert', () => {
    render(<ErrorNotice message={null} />)
    expect(screen.queryByRole('alert')).toBeNull()
  })

  it('message=undefined 不渲染 alert', () => {
    render(<ErrorNotice message={undefined} />)
    expect(screen.queryByRole('alert')).toBeNull()
  })

  it('message="" 不渲染 alert', () => {
    render(<ErrorNotice message="" />)
    expect(screen.queryByRole('alert')).toBeNull()
  })

  it('有 message 渲染 role=alert', () => {
    render(<ErrorNotice message="出错了" />)
    expect(screen.getByRole('alert')).toBeInTheDocument()
  })

  it('渲染错误消息文本', () => {
    render(<ErrorNotice message="网络连接失败" />)
    expect(screen.getByText('网络连接失败')).toBeInTheDocument()
  })

  it('className 传递', () => {
    render(<ErrorNotice message="错误" className="my-extra" />)
    expect(screen.getByRole('alert')).toHaveClass('my-extra')
  })

  it('包含红色样式类', () => {
    render(<ErrorNotice message="错误" />)
    const el = screen.getByRole('alert')
    expect(el.className).toContain('border-red-500/30')
    expect(el.className).toContain('bg-red-500/10')
  })
})

describe('EmptyState', () => {
  it('渲染 title', () => {
    render(<EmptyState title="暂无数据" />)
    expect(screen.getByText('暂无数据')).toBeInTheDocument()
  })

  it('渲染 hint', () => {
    render(<EmptyState title="暂无数据" hint="请检查配置" />)
    expect(screen.getByText('请检查配置')).toBeInTheDocument()
  })

  it('不传 hint 时不渲染提示', () => {
    render(<EmptyState title="暂无数据" />)
    expect(screen.queryByText('提示')).toBeNull()
  })

  it('容器使用 flex 居中布局', () => {
    const { container } = render(<EmptyState title="空状态" />)
    const wrapper = container.firstChild as HTMLElement
    expect(wrapper).toHaveClass('flex', 'flex-col', 'items-center', 'justify-center')
  })
})

describe('Label', () => {
  it('渲染 children', () => {
    render(<Label>用户名</Label>)
    expect(screen.getByText('用户名')).toBeInTheDocument()
  })

  it('传递 htmlFor', () => {
    render(<Label htmlFor="name">用户名</Label>)
    expect(screen.getByText('用户名')).toHaveAttribute('for', 'name')
  })

  it('渲染为 label 元素', () => {
    render(<Label>标签</Label>)
    expect(screen.getByText('标签').tagName).toBe('LABEL')
  })

  it('包含样式类', () => {
    render(<Label>标签</Label>)
    expect(screen.getByText('标签')).toHaveClass('block', 'text-xs', 'font-medium')
  })
})

describe('Button', () => {
  it('渲染 button 元素', () => {
    render(<Button>点击</Button>)
    expect(screen.getByRole('button')).toBeInTheDocument()
  })

  it('渲染 children 文本', () => {
    render(<Button>确认</Button>)
    expect(screen.getByRole('button')).toHaveTextContent('确认')
  })

  it('默认 type 为 button', () => {
    render(<Button>按钮</Button>)
    expect(screen.getByRole('button')).toHaveAttribute('type', 'button')
  })

  it('可覆盖 type', () => {
    render(<Button type="submit">提交</Button>)
    expect(screen.getByRole('button')).toHaveAttribute('type', 'submit')
  })

  it('disabled 状态', () => {
    render(<Button disabled>禁用</Button>)
    expect(screen.getByRole('button')).toBeDisabled()
  })

  it('variant=primary 样式', () => {
    render(<Button variant="primary">主按钮</Button>)
    expect(screen.getByRole('button')).toHaveClass('bg-brand', 'text-white')
  })

  it('variant=ghost 样式', () => {
    render(<Button variant="ghost">幽灵</Button>)
    expect(screen.getByRole('button')).toHaveClass('text-ink-secondary')
  })

  it('variant=outline 样式', () => {
    render(<Button variant="outline">描边</Button>)
    expect(screen.getByRole('button')).toHaveClass('border')
  })

  it('variant=danger 样式', () => {
    render(<Button variant="danger">危险</Button>)
    expect(screen.getByRole('button')).toHaveClass('bg-red-600')
  })

  it('className 传递', () => {
    render(<Button className="my-extra">按钮</Button>)
    expect(screen.getByRole('button')).toHaveClass('my-extra')
  })

  it('点击事件触发', () => {
    const onClick = vi.fn()
    render(<Button onClick={onClick}>点击我</Button>)
    fireEvent.click(screen.getByRole('button'))
    expect(onClick).toHaveBeenCalledTimes(1)
  })

  it('disabled 按钮不触发点击', () => {
    const onClick = vi.fn()
    render(<Button onClick={onClick} disabled>禁用</Button>)
    fireEvent.click(screen.getByRole('button'))
    expect(onClick).not.toHaveBeenCalled()
  })
})

describe('TextInput', () => {
  it('渲染 input 元素', () => {
    render(<TextInput />)
    expect(screen.getByRole('textbox')).toBeInTheDocument()
  })

  it('传递 className', () => {
    render(<TextInput className="w-50" />)
    expect(screen.getByRole('textbox')).toHaveClass('w-50')
  })

  it('设置 value 和 onChange', () => {
    const onChange = vi.fn()
    render(<TextInput value="hello" onChange={onChange} />)
    expect(screen.getByRole('textbox')).toHaveValue('hello')
    const input = screen.getByRole('textbox') as HTMLInputElement
    fireEvent.change(input, { target: { value: 'hello world' } })
    expect(onChange).toHaveBeenCalled()
  })

  it('设置 placeholder', () => {
    render(<TextInput placeholder="请输入" />)
    expect(screen.getByRole('textbox')).toHaveAttribute('placeholder', '请输入')
  })

  it('传递其他 props', () => {
    const { container } = render(<TextInput type="password" maxLength={10} />)
    const input = container.querySelector('input') as HTMLInputElement
    expect(input).toHaveAttribute('type', 'password')
    expect(input).toHaveAttribute('maxlength', '10')
  })

  it('传递 ref', () => {
    const ref = { current: null as HTMLInputElement | null }
    render(<TextInput ref={ref} />)
    expect(ref.current).toBeInstanceOf(HTMLInputElement)
  })
})

describe('TextArea', () => {
  it('渲染 textarea 元素', () => {
    const { container } = render(<TextArea />)
    const ta = container.querySelector('textarea') as HTMLTextAreaElement
    expect(ta.tagName).toBe('TEXTAREA')
  })

  it('传递 className', () => {
    render(<TextArea className="h-40" />)
    expect(screen.getByRole('textbox')).toHaveClass('h-40')
  })

  it('设置 value 和 onChange', () => {
    const onChange = vi.fn()
    render(<TextArea value="text" onChange={onChange} />)
    expect(screen.getByRole('textbox')).toHaveValue('text')
    const ta = screen.getByRole('textbox') as HTMLTextAreaElement
    fireEvent.change(ta, { target: { value: 'text more' } })
    expect(onChange).toHaveBeenCalled()
  })

  it('设置 placeholder', () => {
    render(<TextArea placeholder="请输入内容" />)
    expect(screen.getByRole('textbox')).toHaveAttribute('placeholder', '请输入内容')
  })

  it('传递 ref', () => {
    const ref = { current: null as HTMLTextAreaElement | null }
    render(<TextArea ref={ref} />)
    expect(ref.current).toBeInstanceOf(HTMLTextAreaElement)
  })
})

describe('Select', () => {
  it('渲染 select 元素', () => {
    render(<Select>
      <option value="a">选项A</option>
      <option value="b">选项B</option>
    </Select>)
    expect(screen.getByRole('combobox')).toBeInTheDocument()
  })

  it('渲染 option children', () => {
    render(<Select>
      <option value="a">选项A</option>
      <option value="b">选项B</option>
    </Select>)
    expect(screen.getByRole('option', { name: '选项A' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: '选项B' })).toBeInTheDocument()
  })

  it('传递 className', () => {
    render(<Select className="w-40" />)
    expect(screen.getByRole('combobox')).toHaveClass('w-40')
  })

  it('设置 value', () => {
    render(
      <Select value="b">
        <option value="a">选项A</option>
        <option value="b">选项B</option>
      </Select>,
    )
    expect(screen.getByRole('combobox')).toHaveValue('b')
  })

  it('onChange 触发', () => {
    const onChange = vi.fn()
    render(
      <Select onChange={onChange}>
        <option value="a">选项A</option>
        <option value="b">选项B</option>
      </Select>,
    )
    const select = screen.getByRole('combobox') as HTMLSelectElement
    fireEvent.change(select, { target: { value: 'b' } })
    expect(onChange).toHaveBeenCalled()
  })

  it('disabled 状态', () => {
    render(<Select disabled />)
    expect(screen.getByRole('combobox')).toBeDisabled()
  })
})

describe('Card', () => {
  it('渲染 children', () => {
    render(<Card>卡片内容</Card>)
    expect(screen.getByText('卡片内容')).toBeInTheDocument()
  })

  it('渲染为 div', () => {
    render(<Card>内容</Card>)
    expect(screen.getByText('内容').tagName).toBe('DIV')
  })

  it('包含基础样式类', () => {
    render(<Card>内容</Card>)
    const card = screen.getByText('内容')
    expect(card.className).toContain('rounded-xl')
    expect(card.className).toContain('border')
    expect(card.className).toContain('p-4')
  })

  it('className 传递', () => {
    render(<Card className="my-extra">内容</Card>)
    expect(screen.getByText('内容')).toHaveClass('my-extra')
  })

  it('可以包含多个子元素', () => {
    render(
      <Card>
        <div>第一个</div>
        <div>第二个</div>
      </Card>,
    )
    expect(screen.getByText('第一个')).toBeInTheDocument()
    expect(screen.getByText('第二个')).toBeInTheDocument()
  })
})
