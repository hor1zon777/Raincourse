import type { CSSProperties, ReactNode } from 'react';
import { Typography } from 'antd';

const { Title } = Typography;

interface PageHeaderProps {
  /** 标题文本 */
  title: ReactNode;
  /** 标题前置图标（与标题同行、自动留间距） */
  icon?: ReactNode;
  /** 标题左侧元素，常用于「返回」按钮 */
  before?: ReactNode;
  /** 右侧操作区（按钮、控件等），自动靠右对齐 */
  extra?: ReactNode;
  /** 自定义外层样式（覆盖默认间距时使用） */
  style?: CSSProperties;
}

/**
 * 统一页头：`[before] [icon + 标题] ……(spacer)…… [extra]`。
 *
 * 各页面原先各自手写 `flex + Title level=4 + marginBottom:16`，
 * 标题图标有无、返回按钮位置、间距均不一致；统一到此组件以保证跨页一致性，
 * 并在窄窗口下自动换行（flex-wrap）。
 */
export default function PageHeader({ title, icon, before, extra, style }: PageHeaderProps) {
  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 12,
        marginBottom: 16,
        flexWrap: 'wrap',
        ...style,
      }}
    >
      {before}
      <Title level={4} style={{ margin: 0, display: 'flex', alignItems: 'center', gap: 8 }}>
        {icon}
        {title}
      </Title>
      {extra && (
        <div
          style={{
            marginLeft: 'auto',
            display: 'flex',
            alignItems: 'center',
            gap: 8,
            flexWrap: 'wrap',
          }}
        >
          {extra}
        </div>
      )}
    </div>
  );
}
