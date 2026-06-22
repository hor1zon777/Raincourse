import { useEffect, useRef, useState } from 'react';
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
  InfoCircleOutlined,
} from '@ant-design/icons';
import { useAuthStore } from '../stores/authStore';

const { Sider, Content, Header } = AntLayout;
const { Text } = Typography;

const menuItems = [
  { key: '/', icon: <BookOutlined />, label: '课程列表' },
  { key: '/answers', icon: <FileTextOutlined />, label: '答案文件' },
  { key: '/export', icon: <ExportOutlined />, label: '数据导出' },
  { key: '/settings', icon: <SettingOutlined />, label: '设置' },
  { key: '/about', icon: <InfoCircleOutlined />, label: '关于' },
];

export default function AppLayout() {
  const [collapsed, setCollapsed] = useState(false);
  const navigate = useNavigate();
  const location = useLocation();
  // selector 细粒化
  const userInfo = useAuthStore((s) => s.userInfo);
  const logout = useAuthStore((s) => s.logout);
  const { token } = theme.useToken();

  // 记住「课程」区段最近访问的子路径（列表 `/` 或某课程详情 `/course/:id`）。
  // 这样从别的菜单（设置 / 答案文件…）切回「课程列表」时能回到原来的课程详情，
  // 而非每次都丢失位置、回到「我的课程」列表（课程详情的 UI 选择由 store 按 courseId 持久化，
  // 配合此处的位置记忆即可完整恢复现场）。
  const lastCoursePathRef = useRef('/');
  // 同样记住「答案文件」区段最近访问的子路径，避免从答案预览页切走后再切回时丢失预览位置。
  const lastAnswerPathRef = useRef('/answers');
  useEffect(() => {
    if (location.pathname === '/' || location.pathname.startsWith('/course/')) {
      lastCoursePathRef.current = location.pathname;
    }
    if (location.pathname === '/answers' || location.pathname.startsWith('/answers/')) {
      lastAnswerPathRef.current = location.pathname;
    }
  }, [location.pathname]);

  // 课程详情（/course/:id）属于「课程列表」区段，菜单高亮归到 `/`，避免详情页时无任何高亮
  const selectedKey = location.pathname.startsWith('/course/')
    ? '/'
    : location.pathname.startsWith('/answers/')
      ? '/answers'
      : location.pathname;

  const handleMenuClick = (key: string) => {
    if (key === '/') {
      // 已在课程区段内：回到列表 `/`；在区段外：回到最近浏览的课程页（列表或详情）
      const inCourses =
        location.pathname === '/' || location.pathname.startsWith('/course/');
      navigate(inCourses ? '/' : lastCoursePathRef.current);
      return;
    }
    if (key === '/answers') {
      // 已在答案区段内：回到文件列表；在区段外：回到最近浏览的答案页（列表或预览）。
      const inAnswers =
        location.pathname === '/answers' || location.pathname.startsWith('/answers/');
      navigate(inAnswers ? '/answers' : lastAnswerPathRef.current);
      return;
    }
    navigate(key);
  };

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
          selectedKeys={[selectedKey]}
          items={menuItems}
          onClick={({ key }) => handleMenuClick(key)}
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
                <Avatar
                  size="small"
                  src={userInfo.avatar || undefined}
                  icon={<UserOutlined />}
                />
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
