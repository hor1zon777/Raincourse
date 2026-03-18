import { useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { Table, Typography, Button, Space, Card, Spin } from 'antd';
import { EyeOutlined } from '@ant-design/icons';
import { useCourseStore } from '../stores/courseStore';
import type { Course } from '../types';

const { Title } = Typography;

export default function Dashboard() {
  const navigate = useNavigate();
  const { courses, loading, fetchCourses } = useCourseStore();

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
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
        <Title level={4} style={{ margin: 0 }}>我的课程</Title>
        <Button onClick={fetchCourses} loading={loading}>
          刷新
        </Button>
      </div>

      <Card>
        <Spin spinning={loading}>
          <Table
            columns={columns}
            dataSource={courses}
            rowKey="classroom_id"
            pagination={false}
            size="middle"
          />
        </Spin>
      </Card>
    </div>
  );
}
