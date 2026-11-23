import { ReactNode } from 'react';
import { ChevronDown } from 'lucide-react';
import clsx from 'clsx';
import { useShallow } from 'zustand/react/shallow';
import Text from '../ui/Text';
import VisibilityToggle, { HIDDEN_CONTENT_CLASS } from '../ui/VisibilityToggle';
import { TextVariants } from '../../types/typography';
import { useSettingsStore } from '../../store/useSettingsStore';
import { useCollapsibleHeight } from '../../hooks/useCollapsibleHeight';
import { useToolVisibility } from '../../context/ToolVisibilityContext';
import { withToolCollapsed } from '../../utils/adjustments';

interface AdjustmentSubSectionProps {
  actions?: ReactNode;
  children: ReactNode;
  hasActiveAdjustment?: boolean;
  id: string;
  order: number;
  title: string;
}

export default function AdjustmentSubSection({
  actions,
  children,
  hasActiveAdjustment = false,
  id,
  order,
  title,
}: AdjustmentSubSectionProps) {
  const visibility = useToolVisibility(id);
  const { appSettings, handleSettingsChange } = useSettingsStore(
    useShallow((state) => ({
      appSettings: state.appSettings,
      handleSettingsChange: state.handleSettingsChange,
    })),
  );

  const collapsedTools = appSettings?.adjustmentLayout?.collapsedTools ?? [];
  const isCollapsed = collapsedTools.includes(id);
  const { contentRef, wrapperRef } = useCollapsibleHeight(!isCollapsed);

  const handleToggle = () => {
    if (!appSettings) {
      return;
    }
    handleSettingsChange(withToolCollapsed(appSettings, id, !isCollapsed));
  };

  return (
    <div className="p-1 bg-bg-tertiary rounded-md" style={{ order }}>
      <div className="group/visibility flex items-center gap-2 cursor-pointer select-none" onClick={handleToggle}>
        <Text variant={TextVariants.heading} className={clsx('flex items-center gap-1.5', !visibility && 'grow')}>
          <span>{title}</span>
          {hasActiveAdjustment && <span className="w-1.5 h-1.5 rounded-full bg-accent shrink-0" />}
        </Text>
        {visibility && (
          <div className="grow flex items-center">
            <VisibilityToggle isVisible={visibility.isVisible} onToggle={visibility.toggle} revealOnHover />
          </div>
        )}
        {actions && <div onClick={(e) => e.stopPropagation()}>{actions}</div>}
        <ChevronDown
          className={clsx('text-text-secondary transition-transform duration-300', !isCollapsed && 'rotate-180')}
          size={16}
        />
      </div>
      <div ref={wrapperRef} className="overflow-hidden transition-all duration-300 ease-in-out">
        <div
          className={clsx('pt-2 transition-opacity duration-300', visibility?.isDimmed && HIDDEN_CONTENT_CLASS)}
          ref={contentRef}
        >
          {children}
        </div>
      </div>
    </div>
  );
}
