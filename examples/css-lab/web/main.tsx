// The same pages rendered by react-dom: the reference gpuijs is compared against.
import { createRoot } from 'react-dom/client';

import { App } from '../src/App';

createRoot(document.getElementById('root')!).render(<App />);
