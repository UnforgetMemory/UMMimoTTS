import type {
  ButtonHTMLAttributes,
  InputHTMLAttributes,
  ReactNode,
  Ref,
  SelectHTMLAttributes,
  TextareaHTMLAttributes,
} from 'react'

export function Spinner({ className = '' }: { className?: string }) {
  return (
    <span
      className={`inline-block h-4 w-4 animate-spin rounded-full border-2 border-current border-t-transparent ${className}`}
      aria-hidden="true"
    />
  )
}

export function ErrorNotice({ message, className = '' }: { message: string | null | undefined; className?: string }) {
  if (!message) return null
  return (
    <div
      className={`rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2 text-sm text-red-600 dark:text-red-400 ${className}`}
      role="alert"
    >
      {message}
    </div>
  )
}

export function EmptyState({ title, hint }: { title: string; hint?: string }) {
  return (
    <div className="flex flex-col items-center justify-center gap-1 py-12 text-center">
      <div className="text-sm font-medium text-ink-secondary">{title}</div>
      {hint ? <div className="text-xs text-ink-tertiary">{hint}</div> : null}
    </div>
  )
}

export function Label({ children, htmlFor }: { children: ReactNode; htmlFor?: string }) {
  return (
    <label htmlFor={htmlFor} className="mb-1 block text-xs font-medium text-ink-secondary">
      {children}
    </label>
  )
}

type ButtonVariant = 'primary' | 'ghost' | 'outline' | 'danger'

const BUTTON_VARIANTS: Record<ButtonVariant, string> = {
  primary: 'bg-brand text-white hover:bg-brand-hover active:bg-brand-pressed',
  ghost: 'text-ink-secondary hover:bg-surface-3 hover:text-ink',
  outline: 'border border-border text-ink hover:bg-surface-2',
  danger: 'bg-red-600 text-white hover:bg-red-700',
}

export function Button({
  variant = 'primary',
  className = '',
  type = 'button',
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: ButtonVariant }) {
  return (
    <button
      type={type}
      className={`inline-flex items-center justify-center gap-1.5 rounded-lg px-3 py-1.5 text-sm font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${BUTTON_VARIANTS[variant]} ${className}`}
      {...rest}
    />
  )
}

export function TextInput({
  className = '',
  ref,
  ...rest
}: InputHTMLAttributes<HTMLInputElement> & { ref?: Ref<HTMLInputElement> }) {
  return (
    <input
      ref={ref}
      className={`w-full rounded-lg border border-border bg-surface px-3 py-1.5 text-sm text-ink outline-none placeholder:text-ink-tertiary focus:ring-2 focus:ring-brand-ring ${className}`}
      {...rest}
    />
  )
}

export function TextArea({
  className = '',
  ref,
  ...rest
}: TextareaHTMLAttributes<HTMLTextAreaElement> & { ref?: Ref<HTMLTextAreaElement> }) {
  return (
    <textarea
      ref={ref}
      className={`w-full resize-y rounded-lg border border-border bg-surface px-3 py-2 text-sm text-ink outline-none placeholder:text-ink-tertiary focus:ring-2 focus:ring-brand-ring ${className}`}
      {...rest}
    />
  )
}

export function Select({
  className = '',
  children,
  ...rest
}: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <select
      className={`appearance-none rounded-lg border border-border bg-surface px-3 py-1.5 text-sm text-ink outline-none focus:ring-2 focus:ring-brand-ring ${className}`}
      {...rest}
    >
      {children}
    </select>
  )
}

export function Card({ children, className = '' }: { children: ReactNode; className?: string }) {
  return <div className={`rounded-xl border border-border bg-surface-2 p-4 ${className}`}>{children}</div>
}
