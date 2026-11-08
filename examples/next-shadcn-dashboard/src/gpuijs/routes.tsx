// The app/ directory's routes, which Next.js finds from the file system, for src/main.tsx (gpuijs)
// and web/main.tsx (the browser reference).
import { App, document, server, type Route } from './router';

import RootLayout from '@/app/layout';
import NotFound from '@/app/not-found';
import AuthLayout from '@/app/auth/layout';
import AuthPage from '@/app/auth/page';
import SignInPage from '@/app/auth/sign-in/[[...sign-in]]/page';
import SignUpPage from '@/app/auth/sign-up/[[...sign-up]]/page';
import DashboardLayout from '@/app/dashboard/layout';
import DashboardPage from '@/app/dashboard/page';
import OverviewLayout from '@/app/dashboard/overview/layout';
import AreaStats from '@/app/dashboard/overview/@area_stats/page';
import AreaStatsLoading from '@/app/dashboard/overview/@area_stats/loading';
import BarStats from '@/app/dashboard/overview/@bar_stats/page';
import BarStatsLoading from '@/app/dashboard/overview/@bar_stats/loading';
import PieStats from '@/app/dashboard/overview/@pie_stats/page';
import PieStatsLoading from '@/app/dashboard/overview/@pie_stats/loading';
import Sales from '@/app/dashboard/overview/@sales/page';
import SalesLoading from '@/app/dashboard/overview/@sales/loading';
import ProductPage from '@/app/dashboard/product/page';
import ProductLoading from '@/app/dashboard/product/loading';
import ProductIdPage from '@/app/dashboard/product/[productId]/page';
import UsersPage from '@/app/dashboard/users/page';
import UsersLoading from '@/app/dashboard/users/loading';
import KanbanPage from '@/app/dashboard/kanban/page';
import ChatPage from '@/app/dashboard/chat/page';
import AiChatPage from '@/app/dashboard/ai-chat/page';
import FormsPage from '@/app/dashboard/forms/page';
import BasicFormPage from '@/app/dashboard/forms/basic/page';
import AdvancedFormPage from '@/app/dashboard/forms/advanced/page';
import MultiStepFormPage from '@/app/dashboard/forms/multi-step/page';
import SheetFormPage from '@/app/dashboard/forms/sheet-form/page';
import ReactQueryPage from '@/app/dashboard/react-query/page';
import IconsPage from '@/app/dashboard/elements/icons/page';
import NotificationsPage from '@/app/dashboard/notifications/page';
import ProfilePage from '@/app/dashboard/profile/[[...profile]]/page';
import WorkspacesPage from '@/app/dashboard/workspaces/page';
import TeamPage from '@/app/dashboard/workspaces/team/[[...rest]]/page';
import BillingPage from '@/app/dashboard/billing/page';
import ExclusivePage from '@/app/dashboard/exclusive/page';

const root = (children: React.ReactNode) => server(() => document(RootLayout, children));
const auth = (children: React.ReactNode) => root(server(AuthLayout, { children }));
const dashboard = (children: React.ReactNode) => root(server(DashboardLayout, { children }));
// A page gets its params and searchParams as promises, as in Next.js.
const page = (Page: Parameters<typeof server>[0], Loading?: React.ComponentType) => (params: object, search: object) =>
  dashboard(server(Page, { params: Promise.resolve(params), searchParams: Promise.resolve(search) }, Loading, NotFound));

const routes: Route[] = [
  ['/', () => root(server(DashboardPage))],
  ['/auth', () => auth(server(AuthPage))],
  ['/auth/sign-in/*', () => auth(server(SignInPage))],
  ['/auth/sign-up/*', () => auth(server(SignUpPage))],
  ['/dashboard', page(DashboardPage)],
  [
    '/dashboard/overview',
    () =>
      dashboard(
        server(OverviewLayout, {
          area_stats: server(AreaStats, {}, AreaStatsLoading),
          bar_stats: server(BarStats, {}, BarStatsLoading),
          pie_stats: server(PieStats, {}, PieStatsLoading),
          sales: server(Sales, {}, SalesLoading)
        })
      )
  ],
  ['/dashboard/product', page(ProductPage, ProductLoading)],
  ['/dashboard/product/:productId', page(ProductIdPage)],
  ['/dashboard/users', page(UsersPage, UsersLoading)],
  ['/dashboard/kanban', page(KanbanPage)],
  ['/dashboard/chat', page(ChatPage)],
  ['/dashboard/ai-chat', page(AiChatPage)],
  ['/dashboard/forms', page(FormsPage)],
  ['/dashboard/forms/basic', page(BasicFormPage)],
  ['/dashboard/forms/advanced', page(AdvancedFormPage)],
  ['/dashboard/forms/multi-step', page(MultiStepFormPage)],
  ['/dashboard/forms/sheet-form', page(SheetFormPage)],
  ['/dashboard/react-query', page(ReactQueryPage)],
  ['/dashboard/elements/icons', page(IconsPage)],
  ['/dashboard/notifications', page(NotificationsPage)],
  ['/dashboard/profile/*', page(ProfilePage)],
  ['/dashboard/workspaces', page(WorkspacesPage)],
  ['/dashboard/workspaces/team/*', page(TeamPage)],
  ['/dashboard/billing', page(BillingPage)],
  ['/dashboard/exclusive', page(ExclusivePage)]
];

export const app = <App routes={routes} fallback={() => root(<NotFound />)} />;
