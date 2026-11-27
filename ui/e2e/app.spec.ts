import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

async function expectNoA11yViolations(page: Page) {
  const results = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa']).analyze();
  expect(results.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(' ')).join(', ')}`)).toEqual([]);
}

async function joinRoom(page: Page, query = '?mock=1') {
  await page.goto(`/${query}`);
  await page.getByLabel('Nome', { exact: true }).fill('Ana');
  await page.getByRole('button', { name: 'Entrar' }).click();
  await expect(page.getByRole('heading', { level: 1, name: 'Call 1' })).toBeVisible();
}

test('tela de entrada é acessível e valida o formulário', async ({ page }) => {
  await page.goto('/?mock=1');
  await expect(page.getByRole('heading', { name: 'μChat' })).toBeVisible();
  await expect(page.getByLabel('Servidor', { exact: true })).toHaveCount(0);
  await expectNoA11yViolations(page);

  await page.getByRole('button', { name: 'Entrar' }).click();
  const nome = page.getByLabel('Nome', { exact: true });
  await expect(nome).toHaveAttribute('aria-invalid', 'true');
  await expect(nome).toBeFocused();
  await expectNoA11yViolations(page);
});

test('fluxo completo: entrar, ver participantes, mutar, compartilhar e sair', async ({ page }) => {
  await joinRoom(page);

  const people = page.getByRole('list', { name: /Pessoas na sala/ });
  await expect(people.getByRole('listitem')).toHaveCount(5);
  await expect(people.getByRole('listitem').first()).toContainText('Ana');
  await expect(page.getByText('Reconectando…')).toHaveCount(0);
  // O mock faz alguém falar em poucos ticks.
  await expect(page.locator('[data-testid="participante"][data-speaking="true"]').first()).toBeVisible({ timeout: 10_000 });
  await expectNoA11yViolations(page);

  const mute = page.getByRole('button', { name: 'Mudo' });
  await mute.click();
  await expect(mute).toHaveAttribute('aria-pressed', 'true');
  await expect(people.getByRole('listitem').first()).toHaveAttribute('data-muted', 'true');
  await expect(page.locator('[aria-live="polite"]')).toHaveText('Microfone desligado.');
  await page.keyboard.press('Control+Shift+M');
  await expect(mute).toHaveAttribute('aria-pressed', 'false');

  await page.getByRole('button', { name: 'Compartilhar tela' }).click();
  const dialog = page.getByRole('dialog', { name: 'Compartilhar tela' });
  await expect(dialog).toBeVisible();
  await dialog.getByRole('radio', { name: /^Jogo/ }).check();
  await dialog.getByRole('radio', { name: /1080p · 60/ }).check();
  await expectNoA11yViolations(page);
  await dialog.getByRole('button', { name: 'Compartilhar tela' }).click();
  await expect(dialog).toBeHidden();

  await expect(page.getByRole('heading', { name: 'Sua tela' })).toBeVisible();
  await expect(page.getByTestId('screen-video')).toHaveClass(/visivel/, { timeout: 10_000 });
  await expect(page.getByText(/1920×1080/)).toBeVisible();
  const painted = await page
    .getByTestId('screen-video')
    .evaluate((v: HTMLVideoElement) => v.srcObject instanceof MediaStream);
  expect(painted).toBe(true);

  await page.getByRole('button', { name: 'Parar de compartilhar' }).click();
  await expect(page.getByRole('heading', { name: 'Sua tela' })).toBeHidden();
  await expect(page.getByRole('button', { name: 'Compartilhar tela' })).toBeVisible();

  await page.getByRole('button', { name: 'Sair da call' }).click();
  await expect(page.getByRole('button', { name: 'Entrar' })).toBeVisible();
  await expect(page.getByLabel('Nome', { exact: true })).toHaveValue('Ana');
});

test('troca de sala mantém o mudo', async ({ page }) => {
  await joinRoom(page);

  await page.getByRole('button', { name: 'Mudo' }).click();
  await expect(page.getByRole('button', { name: 'Mudo' })).toHaveAttribute('aria-pressed', 'true');

  await page.getByRole('button', { name: 'Call 1' }).click();
  await page.getByRole('menuitemradio', { name: 'Call 2' }).click();

  await expect(page.getByRole('heading', { level: 1, name: 'Call 2' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Mudo' })).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByRole('button', { name: 'Entrar' })).toHaveCount(0);
});

test('visualizador mostra a tela de outra pessoa e entra em tela cheia', async ({ page }) => {
  await joinRoom(page, '?mock=1&remoteShare=1');
  await expect(page.getByRole('heading', { name: 'Tela de Bia Rocha' })).toBeVisible();
  await expect(page.getByTestId('screen-video')).toHaveClass(/visivel/, { timeout: 10_000 });

  await page.getByRole('button', { name: 'Tela cheia' }).click();
  await expect(page.getByRole('button', { name: 'Sair da tela cheia' })).toBeVisible();
  expect(await page.evaluate(() => !!document.fullscreenElement)).toBe(true);
  await page.getByRole('button', { name: 'Sair da tela cheia' }).click();
  await expect(page.getByRole('button', { name: 'Tela cheia' })).toBeVisible();
  await expectNoA11yViolations(page);
});

test('respeita prefers-reduced-motion', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await joinRoom(page);
  const brilho = page.locator('[data-testid="participante"] .brilho').first();
  await expect(brilho.locator('canvas')).toHaveCount(0);
  // `.brilho` não tem transição própria; quem anima a posição na roda é o
  // <li class="blob"> (transform, 650ms normalmente) — é ele que precisa
  // respeitar `prefers-reduced-motion`, não o elemento do glow.
  const blob = page.locator('li.blob').first();
  const duration = await blob.evaluate((el) => parseFloat(getComputedStyle(el).transitionDuration));
  expect(duration).toBeLessThan(0.01);
});

test('clicar no avatar de quem compartilha no palco troca a tela assistida', async ({ page }) => {
  // Dois compartilhando ao mesmo tempo: Bia Rocha (remota, desde a entrada)
  // e o local (Ana), que passa a compartilhar depois. `assistindo` só troca
  // quando a pessoa assistida sai da lista, então o palco continua mostrando
  // Bia Rocha — Ana aparece como o blob pequeno com AO VIVO ao lado dela.
  await joinRoom(page, '?mock=1&remoteShare=1');
  await expect(page.getByRole('heading', { name: 'Tela de Bia Rocha' })).toBeVisible();

  await page.getByRole('button', { name: 'Compartilhar tela' }).click();
  const dialog = page.getByRole('dialog', { name: 'Compartilhar tela' });
  await dialog.getByRole('button', { name: 'Compartilhar tela' }).click();
  await expect(dialog).toBeHidden();
  await expect(page.getByRole('heading', { name: 'Tela de Bia Rocha' })).toBeVisible();

  const blobAna = page.locator('[data-testid="participante"]', { hasText: 'Ana' });
  await expect(blobAna).toBeVisible();
  // Clica na imagem do avatar (não no selo "AO VIVO"): o alvo de clique
  // precisa cobrir o blob inteiro, não só o selinho de 7px.
  await blobAna.locator('.imagem').click();
  await expect(page.getByRole('heading', { name: 'Sua tela' })).toBeVisible();
});

test('roda → Ampliar tela por teclado volta ao palco', async ({ page }) => {
  await joinRoom(page, '?mock=1&remoteShare=1');
  await expect(page.getByRole('heading', { name: 'Tela de Bia Rocha' })).toBeVisible();

  await page.getByRole('button', { name: 'Roda' }).click();
  const ampliar = page.getByRole('button', { name: 'Ampliar tela' });
  await expect(ampliar).toBeVisible();
  await ampliar.focus();
  await page.keyboard.press('Enter');

  await expect(ampliar).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Roda' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Tela de Bia Rocha' })).toBeVisible();
});

test('configurações: supressão de ruído, sensibilidade e medidor ao vivo', async ({ page }) => {
  await joinRoom(page);
  await page.getByRole('button', { name: 'Configurações' }).click();
  const dialog = page.getByRole('dialog', { name: 'Configurações' });
  await expect(dialog).toBeVisible();

  const supressao = dialog.getByRole('checkbox', { name: 'Supressão de ruído' });
  await expect(supressao).toBeChecked();
  await supressao.uncheck();
  await expect(supressao).not.toBeChecked();

  // Sem corte por padrão: o medidor aparece, a marca do limiar não.
  const medidor = dialog.getByRole('meter', { name: 'Nível do microfone' });
  await expect(dialog.getByText('Sensibilidade do microfone: sem corte')).toBeVisible();
  await expect(dialog.locator('[data-limiar]')).toHaveCount(0);

  // O mock faz o nível oscilar: o valor anunciado muda sozinho.
  const primeiro = await medidor.getAttribute('aria-valuenow');
  await expect.poll(() => medidor.getAttribute('aria-valuenow')).not.toBe(primeiro);

  const sensibilidade = dialog.getByRole('slider', { name: /Sensibilidade do microfone/ });
  await sensibilidade.focus();
  await page.keyboard.press('ArrowLeft');
  await expect(dialog.getByText('Sensibilidade do microfone: 95%')).toBeVisible();
  await expect(dialog.locator('[data-limiar]')).toHaveCount(1);
  await expectNoA11yViolations(page);

  // Os ajustes ficam salvos para a próxima abertura do app.
  const salvo = await page.evaluate(() => JSON.parse(localStorage.getItem('sheeshcord:audio') ?? 'null'));
  expect(salvo).toEqual({ noiseSuppression: false, micSensitivity: 95, micVolume: 100 });
});
