import { useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import FirstLaunchOnboarding from '../src/components/FirstLaunchOnboarding';
import '../src/App.css';

const params = new URLSearchParams(location.search);
const isCase = location.pathname === '/case';
if (isCase) {
  localStorage.setItem('ai-os.beta7.first-launch', '2');
  if (params.has('large')) document.body.style.fontSize = '24px';
}

function Case() {
  const [complete, setComplete] = useState(false);
  useEffect(() => {
    const timer = setInterval(() => {
      const button = [...document.querySelectorAll('button')].find(b => b.textContent === '完成设置，开始聊天');
      if (!button) return;
      const rect = button.getBoundingClientRect();
      const styles = getComputedStyle(button);
      const center = { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
      const hit = document.elementFromPoint(center.x, center.y);
      const card = document.querySelector('.first-launch-card')!;
      const cardRect = card.getBoundingClientRect();
      parent.postMessage({ case: params.get('name'), complete, exists: true,
        viewport: { width: innerWidth, height: innerHeight },
        rect: rect.toJSON(), card: cardRect.toJSON(),
        styles: Object.fromEntries(['display', 'opacity', 'visibility', 'position', 'zIndex', 'pointerEvents'].map(key => [key, styles[key as keyof CSSStyleDeclaration]])),
        reachable: rect.top >= 0 && rect.bottom <= innerHeight && rect.left >= 0 && rect.right <= innerWidth && (hit === button || button.contains(hit)),
        progress: localStorage.getItem('ai-os.beta7.first-launch'),
      }, location.origin);
    }, 200);
    return () => clearInterval(timer);
  }, [complete]);
  if (complete) return <h1 role="status">Fixture setup completed</h1>;
  return <FirstLaunchOnboarding onOpenMyAi={() => {}} onOpenAgents={() => {}}
    onStartChat={() => { setComplete(true); parent.postMessage({ case: params.get('name'), completed: true, progress: localStorage.getItem('ai-os.beta7.first-launch') }, location.origin); }} />;
}

function Matrix() {
  const [results, setResults] = useState<Record<string, any>>({});
  useEffect(() => {
    const receive = (event: MessageEvent) => {
      if (event.origin !== location.origin || !event.data?.case) return;
      setResults(previous => {
        const before = previous[event.data.case];
        const result = { ...before, ...event.data, initialReachable: before?.initialReachable ?? event.data.reachable };
        return JSON.stringify(before) === JSON.stringify(result) ? previous : { ...previous, [event.data.case]: result };
      });
    };
    window.addEventListener('message', receive);
    return () => window.removeEventListener('message', receive);
  }, []);
  const cases = ['600x450', '900x500', '1100x720', '600x450-large'];
  const ready = cases.every(name => results[name]?.initialReachable && (results[name]?.reachable || results[name]?.completed))
    && results['600x450']?.completed && results['600x450'].progress === 'complete';
  useEffect(() => {
    if (ready) void fetch('/receipt', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(results) });
  }, [ready, results]);
  return <main style={{ overflow: 'auto', height: '100vh', padding: 16 }}>
    <h1>Beta 7 Setup layout regression — isolated platform</h1>
    <p role="status">{ready ? 'PASS: beta7_setup_layout — all viewports and real completion click' : 'WAIT: viewport checks and click completion in the 600x450 frame'}</p>
    <pre id="layout-results" style={{ whiteSpace: 'pre-wrap' }}>{JSON.stringify(results, null, 2)}</pre>
    {[[600, 450], [900, 500], [1100, 720]].map(([width, height]) => <section key={width}>
      <h2>{width} × {height}</h2>
      <iframe title={`Setup ${width}x${height}`} src={`/case?name=${width}x${height}`} style={{ width, height, border: '1px solid white' }} />
    </section>)}
    <h2>600 × 450 — 150% text and long diagnostics</h2>
    <iframe title="Setup 600x450-large" src="/case?name=600x450-large&large=1" style={{ width: 600, height: 450, border: '1px solid white' }} />
  </main>;
}

createRoot(document.getElementById('root')!).render(isCase ? <Case /> : <Matrix />);
