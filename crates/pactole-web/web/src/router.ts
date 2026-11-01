import { createRouter, createWebHistory } from 'vue-router'

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      name: 'dashboard',
      component: () => import('@/views/DashboardView.vue'),
      meta: { titleKey: 'nav.dashboard' },
    },
    {
      path: '/balance',
      name: 'balance',
      component: () => import('@/views/BalanceView.vue'),
      meta: { titleKey: 'nav.balance' },
    },
    {
      path: '/income',
      name: 'income',
      component: () => import('@/views/IncomeView.vue'),
      meta: { titleKey: 'nav.income' },
    },
    {
      path: '/expenses',
      name: 'expenses',
      component: () => import('@/views/ExpensesView.vue'),
      meta: { titleKey: 'nav.expenses' },
    },
    {
      path: '/register',
      name: 'register',
      component: () => import('@/views/RegisterView.vue'),
      meta: { titleKey: 'nav.register' },
    },
    {
      path: '/diagnostics',
      name: 'diagnostics',
      component: () => import('@/views/DiagnosticsView.vue'),
      meta: { titleKey: 'nav.diagnostics' },
    },
    {
      path: '/:pathMatch(.*)*',
      name: 'not-found',
      component: () => import('@/views/NotFoundView.vue'),
      meta: { titleKey: 'errors.not-found' },
    },
  ],
  scrollBehavior: () => ({ top: 0 }),
})
