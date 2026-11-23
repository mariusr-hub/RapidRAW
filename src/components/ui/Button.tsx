import clsx from 'clsx';
import type { ButtonHTMLAttributes, ReactNode } from 'react';

type ButtonVariant = 'primary' | 'secondary' | 'outline' | 'ghost' | 'danger';
type ButtonSize = 'sm' | 'small' | 'md' | 'medium' | 'lg' | 'large';

interface ButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'children' | 'size'> {
  children: ReactNode;
  size?: ButtonSize | string;
  variant?: ButtonVariant | string;
}

const VARIANT_CLASSES: Record<ButtonVariant, string> = {
  primary: 'bg-accent text-button-text hover:opacity-90',
  secondary: 'bg-surface text-text-primary border border-border-color hover:bg-card-active',
  outline: 'bg-transparent text-text-primary border border-border-color hover:bg-surface',
  ghost: 'bg-transparent text-text-primary hover:bg-surface/80',
  danger: 'bg-red-600 text-white hover:bg-red-700',
};

const SIZE_CLASSES: Record<ButtonSize, string> = {
  sm: 'py-1 px-2.5 text-xs',
  small: 'py-1 px-2.5 text-xs',
  md: 'py-2 px-4 text-sm',
  medium: 'py-2 px-4 text-sm',
  lg: 'py-2.5 px-5 text-base',
  large: 'py-2.5 px-5 text-base',
};

const Button = ({
  children,
  onClick,
  disabled,
  className = '',
  size = 'md',
  variant = 'primary',
  ...props
}: ButtonProps) => {
  const resolvedVariant = (variant in VARIANT_CLASSES ? variant : 'primary') as ButtonVariant;
  const resolvedSize = (size in SIZE_CLASSES ? size : 'md') as ButtonSize;
  const hasExplicitBg = /\bbg-[^\s]+/.test(className);
  const hasExplicitText = /\btext-(white|black|button-text|text-|accent|red-|emerald-|amber-|sky-|zinc-|neutral-|slate-)/.test(
    className,
  );

  const variantClasses = VARIANT_CLASSES[resolvedVariant]
    .split(/\s+/)
    .filter((token) => {
      if (hasExplicitBg && token.startsWith('bg-')) return false;
      if (hasExplicitText && token.startsWith('text-')) return false;
      return true;
    })
    .join(' ');

  const combinedClasses = clsx(
    'inline-flex items-center justify-center gap-2 font-semibold rounded-md transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed',
    SIZE_CLASSES[resolvedSize],
    variantClasses,
    className,
  );

  return (
    <button onClick={onClick} disabled={disabled} className={combinedClasses} {...props}>
      {children}
    </button>
  );
};

export default Button;
