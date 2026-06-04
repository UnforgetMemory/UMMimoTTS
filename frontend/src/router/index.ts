import { createRouter, createWebHistory, type RouteRecordRaw } from 'vue-router'

const routes: RouteRecordRaw[] = [
  {
    path: '/',
    redirect: '/tasks/single',
  },
  {
    path: '/config',
    name: 'config',
    component: () => import('@/views/ConfigPage.vue'),
  },
  {
    path: '/synthesize',
    name: 'synthesize',
    component: () => import('@/components/SynthesizeForm.vue'),
  },
  {
    path: '/tasks/single',
    name: 'tasks-single',
    component: () => import('@/views/TaskTablePage.vue'),
  },
  {
    path: '/tasks/batch',
    name: 'tasks-batch',
    component: () => import('@/views/BatchTaskTablePage.vue'),
  },
  {
    path: '/tasks/:id',
    name: 'task-detail',
    component: () => import('@/views/TaskDetailPage.vue'),
    props: true,
  },
  {
    path: '/groups/:id',
    name: 'group-detail',
    component: () => import('@/views/GroupDetailPage.vue'),
    props: true,
  },
]

const router = createRouter({
  history: createWebHistory(),
  routes,
  scrollBehavior(to, _from, savedPosition) {
    if (savedPosition) {
      return savedPosition
    }
    if (to.hash) {
      return { el: to.hash, behavior: 'smooth' }
    }
    return { top: 0 }
  },
})

export default router
