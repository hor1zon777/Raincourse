import { lazy, Suspense } from 'react';
import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';
import { ConfigProvider, App as AntApp, Spin } from 'antd';
import zhCN from 'antd/locale/zh_CN';
import AppLayout from './components/Layout';
import { useAuthStore } from './stores/authStore';

// 懒加载所有路由：登录页就不会把课程详情/刷课/导出页全打进主 bundle
const Login = lazy(() => import('./routes/Login'));
const Dashboard = lazy(() => import('./routes/Dashboard'));
const CourseDetail = lazy(() => import('./routes/CourseDetail'));
const StudyProgress = lazy(() => import('./routes/StudyProgress'));
const AnswerFiles = lazy(() => import('./routes/AnswerFiles'));
const ExamExport = lazy(() => import('./routes/ExamExport'));
const Settings = lazy(() => import('./routes/Settings'));
const About = lazy(() => import('./routes/About'));

function PageFallback() {
  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        minHeight: '60vh',
      }}
    >
      <Spin tip="加载中..." size="large">
        <div style={{ minWidth: 120, minHeight: 60 }} />
      </Spin>
    </div>
  );
}

function ProtectedRoute({ children }: { children: React.ReactNode }) {
  // selector 细粒化：只订阅 isLoggedIn，其它字段变化不会重渲染整个路由树
  const isLoggedIn = useAuthStore((s) => s.isLoggedIn);
  if (!isLoggedIn) {
    return <Navigate to="/login" replace />;
  }
  return <>{children}</>;
}

export default function App() {
  return (
    <ConfigProvider
      locale={zhCN}
      theme={{
        token: {
          colorPrimary: '#1677ff',
          borderRadius: 6,
        },
      }}
    >
      <AntApp>
        <BrowserRouter>
          <Suspense fallback={<PageFallback />}>
            <Routes>
              <Route path="/login" element={<Login />} />
              <Route
                element={
                  <ProtectedRoute>
                    <AppLayout />
                  </ProtectedRoute>
                }
              >
                <Route path="/" element={<Dashboard />} />
                <Route path="/course/:id" element={<CourseDetail />} />
                <Route path="/study/:id" element={<StudyProgress />} />
                <Route path="/answers" element={<AnswerFiles />} />
                <Route path="/export" element={<ExamExport />} />
                <Route path="/settings" element={<Settings />} />
                <Route path="/about" element={<About />} />
              </Route>
              <Route path="*" element={<Navigate to="/" replace />} />
            </Routes>
          </Suspense>
        </BrowserRouter>
      </AntApp>
    </ConfigProvider>
  );
}
