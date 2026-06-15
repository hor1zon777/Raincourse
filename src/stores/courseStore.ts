import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import type { Course, Work, Ppt } from '../types';
import { normalizeError } from '../utils/errors';

/**
 * 课程详情页的 UI 选择状态（按 courseId 持久化）。
 *
 * 提升到 store 而非组件 useState：切换 antd Tab / 组件重渲染或重挂载时，
 * 勾选、筛选、当前 Tab 都不丢失；按 courseId 分桶，切换课程天然隔离。
 */
export interface CourseUI {
  activeTab: string;
  selectedTaskIds: number[];
  selectedQuizIds: number[];
  typeFilter: number[];
  nameFilter: string;
}

export const DEFAULT_COURSE_UI: CourseUI = {
  activeTab: 'works',
  selectedTaskIds: [],
  selectedQuizIds: [],
  typeFilter: [],
  nameFilter: '',
};

interface CourseState {
  courses: Course[];
  works: Work[];
  ppts: Ppt[];
  coursesLoading: boolean;
  worksLoading: boolean;
  pptsLoading: boolean;
  error: string | null;
  // 当前 works/ppts 对应的 courseId，切换课程时用来判断是否需要清空
  currentCourseId: string | null;
  // 按 courseId 持久化的页面 UI 选择状态
  courseUI: Record<string, CourseUI>;

  fetchCourses: () => Promise<void>;
  fetchWorks: (courseId: string) => Promise<void>;
  fetchPpts: (courseId: string) => Promise<void>;
  clearWorks: () => void;
  setCourseContext: (courseId: string) => void;
  patchCourseUI: (courseId: string, patch: Partial<CourseUI>) => void;
  resetCourseUI: (courseId: string) => void;
}

export const useCourseStore = create<CourseState>((set, get) => ({
  courses: [],
  works: [],
  ppts: [],
  coursesLoading: false,
  worksLoading: false,
  pptsLoading: false,
  error: null,
  currentCourseId: null,
  courseUI: {},

  fetchCourses: async () => {
    set({ coursesLoading: true, error: null });
    try {
      const courses = await invoke<Course[]>('get_course_list');
      set({ courses, coursesLoading: false });
    } catch (e) {
      set({ coursesLoading: false, error: normalizeError(e).message });
    }
  },

  fetchWorks: async (courseId: string) => {
    set({ worksLoading: true, error: null });
    try {
      const works = await invoke<Work[]>('get_course_works', { courseId });
      // 异步竞争：如果 await 期间课程已切换，不要写回旧数据
      if (get().currentCourseId !== courseId) {
        set({ worksLoading: false });
        return;
      }
      set({ works, worksLoading: false });
    } catch (e) {
      set({ worksLoading: false, error: normalizeError(e).message });
    }
  },

  fetchPpts: async (courseId: string) => {
    set({ pptsLoading: true, error: null });
    try {
      const ppts = await invoke<Ppt[]>('get_course_ppts', { courseId });
      if (get().currentCourseId !== courseId) {
        set({ pptsLoading: false });
        return;
      }
      set({ ppts, pptsLoading: false });
    } catch (e) {
      set({ pptsLoading: false, error: normalizeError(e).message });
    }
  },

  clearWorks: () => {
    set({ works: [], ppts: [] });
  },

  /**
   * 切换到新课程：清空旧数据，记录新 courseId。
   * 必须在 fetchWorks/fetchPpts 之前调用。
   */
  setCourseContext: (courseId: string) => {
    const prev = get().currentCourseId;
    if (prev !== courseId) {
      set({ works: [], ppts: [], currentCourseId: courseId });
    }
  },

  /** 合并更新某课程的 UI 选择状态（不存在则基于默认值创建）。 */
  patchCourseUI: (courseId: string, patch: Partial<CourseUI>) => {
    set((s) => ({
      courseUI: {
        ...s.courseUI,
        [courseId]: { ...(s.courseUI[courseId] ?? DEFAULT_COURSE_UI), ...patch },
      },
    }));
  },

  /** 重置某课程的 UI 选择状态为默认值。 */
  resetCourseUI: (courseId: string) => {
    set((s) => ({
      courseUI: { ...s.courseUI, [courseId]: { ...DEFAULT_COURSE_UI } },
    }));
  },
}));
