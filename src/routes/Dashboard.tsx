import { useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { Table, Button, Space, Card, Spin } from 'antd';
import { EyeOutlined } from '@ant-design/icons';
import { useCourseStore } from '../stores/courseStore';
import PageHeader from '../components/PageHeader';
import type { Course } from '../types';

export default function Dashboard() {
  const navigate = useNavigate();
  // 细粒度 selector，避免 works/ppts 等无关字段变化引起重渲染
  const courses = useCourseStore((s) => s.courses);
  const loading = useCourseStore((s) => s.coursesLoading);
  const fetchCourses = useCourseStore((s) => s.fetchCourses);

  useEffect(() => {
    fetchCourses();
  }, [fetchCourses]);

  const columns = [
    {
      title: '序号',
      dataIndex: 'index',
      key: 'index',
      width: 60,
      render: (_: unknown, __: unknown, index: number) => index + 1,
    },
    {
      title: '课程 ID',
      dataIndex: 'course_id',
      key: 'course_id',
      width: 100,
    },
    {
      title: '课程名称',
      dataIndex: 'course_name',
      key: 'course_name',
    },
    {
      title: '教师',
      dataIndex: 'teacher_name',
      key: 'teacher_name',
      width: 120,
    },
    {
      title: '操作',
      key: 'action',
      width: 100,
      render: (_: unknown, record: Course) => (
        <Space>
          <Button
            type="link"
            size="small"
            icon={<EyeOutlined />}
            onClick={() => navigate(`/course/${record.classroom_id}`)}
          >
            查看
          </Button>
        </Space>
      ),
    },
  ];

  return (
    <div>
      <PageHeader
        title="我的课程"
        extra={
          <Button onClick={fetchCourses} loading={loading}>
            刷新
          </Button>
        }
      />

      <Card>
        <Spin spinning={loading}>
          <Table
            columns={columns}
            dataSource={courses}
            rowKey="classroom_id"
            pagination={false}
            size="middle"
            scroll={{ x: 'max-content' }}
          />
        </Spin>
      </Card>
    </div>
  );
}
