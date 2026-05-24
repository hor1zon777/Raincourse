import { useState } from 'react';
import { Outlet, useNavigate, useLocation } from 'react-router-dom';
import { Layout as AntLayout, Menu, Typography, Avatar, Dropdown, Space, theme } from 'antd';
import {
  BookOutlined,
  FileTextOutlined,
  ExportOutlined,
  SettingOutlined,
  PlayCircleOutlined,
  UserOutlined,
  LogoutOutlined,
} from '@ant-design/icons';
import { useAuthStore } from '../stores/authStore';

const { Sider, Content, Header } = AntLayout;
const { Text } = Typography;

const menuItems = [
  { key: '/', icon: <BookOutlined />, label: '课程列表' },
  { key: '/answers', icon: <FileTextOutlined />, label: '答案文件' },
  { key: '/export', icon: <ExportOutlined />, label: '数据导出' },
  { key: '/settings', icon: <SettingOutlined />, label: '设置' },
];

export default function AppLayout() {
  const [collapsed, setCollapsed] = useState(false);
  const navigate = useNavigate();
  const location = useLocation();
  // selector 细粒化
  const userInfo = useAuthStore((s) => s.userInfo);
  const logout = useAuthStore((s) => s.logout);
  const { token } = theme.useToken();

  const handleSwitchOrLogout = async () => {
    await logout();
    navigate('/login');
  };

  const userMenu = {
    items: [
      {
        key: 'switch',
        icon: <UserOutlined />,
        label: '切换用户',
        onClick: handleSwitchOrLogout,
      },
      {
        key: 'logout',
        icon: <LogoutOutlined />,
        label: '退出登录',
        danger: true,
        onClick: handleSwitchOrLogout,
      },
    ],
  };

  return (
    <AntLayout style={{ height: '100vh', overflow: 'hidden' }}>
      <Sider
        collapsible
        collapsed={collapsed}
        onCollapse={setCollapsed}
        theme="light"
        style={{ height: '100vh', borderRight: `1px solid ${token.colorBorderSecondary}` }}
      >
        <div style={{ padding: '16px', textAlign: 'center' }}>
          <PlayCircleOutlined style={{ fontSize: 28, color: token.colorPrimary }} />
          {!collapsed && (
            <Text strong style={{ display: 'block', marginTop: 4, fontSize: 14 }}>
              雨课堂助手
            </Text>
          )}
        </div>
        <Menu
          mode="inline"
          selectedKeys={[location.pathname]}
          items={menuItems}
          onClick={({ key }) => navigate(key)}
        />
      </Sider>
      <AntLayout style={{ height: '100%' }}>
        <Header
          style={{
            background: token.colorBgContainer,
            padding: '0 24px',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'flex-end',
            borderBottom: `1px solid ${token.colorBorderSecondary}`,
          }}
        >
          {userInfo && (
            <Dropdown menu={userMenu} placement="bottomRight">
              <Space style={{ cursor: 'pointer' }}>
                <Avatar size="small" icon={<UserOutlined />} />
                <Text>{userInfo.name}</Text>
              </Space>
            </Dropdown>
          )}
        </Header>
        <Content
          style={{
            flex: 1,
            minHeight: 0,
            margin: 16,
            padding: 24,
            background: token.colorBgContainer,
            borderRadius: 8,
            overflow: 'auto',
          }}
        >
          <Outlet />
        </Content>
      </AntLayout>
    </AntLayout>
  );
}
