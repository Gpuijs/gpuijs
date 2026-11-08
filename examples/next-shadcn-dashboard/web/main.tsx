// The same app rendered by react-dom, as a reference for the gpuijs window.
import './process';
import { createRoot } from 'react-dom/client';
import { app } from '@/gpuijs/routes';

createRoot(document.getElementById('root')!).render(app);
