import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

async function expectNoA11yViolations(page: Page) {
  const results = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa']).analyze();
  expect(results.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(' ')).join(', ')}`)).toEqual([]);
}

async function joinRoom(page: Page, query = '?mock=1') {
  await page.goto(`/${query}`);
  await page.getByLabel('Servidor', { exact: true }).fill('ws://26.1.2.3:7880');
  await page.getByLabel('Sala', { exact: true }).fill('resenha');
  await page.getByLabel('Seu nome', { exact: true }).fill('Ana');
  await page.getByRole('button', { name: 'Entrar na sala' }).click();
  await expect(page.getByRole('heading', { level: 1, name: 'resenha' })).toBeVisible();
}

test('tela de entrada é acessível e valida o formulário', async ({ page }) => {
  await page.goto('/?mock=1');
  await expect(page.getByRole('heading', { name: 'Sheeshcord' })).toBeVisible();
  await expectNoA11yViolations(page);

  await page.getByRole('button', { name: 'Entrar na sala' }).click();
  await expect(page.getByLabel('Servidor', { exact: true })).toHaveAttribute('aria-invalid', 'true');
  await expect(page.getByLabel('Servidor', { exact: true })).toBeFocused();
  await expect(page.getByText('Informe o nome da sala.')).toBeVisible();
  await expectNoA11yViolations(page);
});

test('fluxo completo: entrar, ver participantes, mutar, compartilhar e sair', async ({ page }) => {
  await joinRoom(page);

  const people = page.getByRole('list', { name: /Pessoas na sala/ });
  await expect(people.getByRole('listitem')).toHaveCount(5);
  await expect(people.getByRole('listitem').first()).toContainText('Ana');
  await expect(page.getByText('Conectado')).toBeVisible();
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
  await expect(page.getByTestId('screen-canvas')).toHaveClass(/visivel/, { timeout: 10_000 });
  await expect(page.getByText('1920×1080')).toBeVisible();
  const painted = await page.getByTestId('screen-canvas').evaluate((c: HTMLCanvasElement) => {
    const px = c.getContext('2d')!.getImageData(c.width / 2, c.height / 2, 1, 1).data;
    return px[3] > 0;
  });
  expect(painted).toBe(true);

  await page.getByRole('button', { name: 'Parar de compartilhar' }).click();
  await expect(page.getByRole('heading', { name: 'Sua tela' })).toBeHidden();
  await expect(page.getByRole('button', { name: 'Compartilhar tela' })).toBeVisible();

  await page.getByRole('button', { name: 'Sair' }).click();
  await expect(page.getByRole('button', { name: 'Entrar na sala' })).toBeVisible();
  await expect(page.getByLabel('Sala', { exact: true })).toHaveValue('resenha');
});

test('visualizador mostra a tela de outra pessoa e entra em tela cheia', async ({ page }) => {
  await joinRoom(page, '?mock=1&remoteShare=1');
  await expect(page.getByRole('heading', { name: 'Tela de Bia Rocha' })).toBeVisible();
  await expect(page.getByTestId('screen-canvas')).toHaveClass(/visivel/, { timeout: 10_000 });

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
  const duration = await page
    .locator('[data-testid="participante"] .anel')
    .first()
    .evaluate((el) => parseFloat(getComputedStyle(el).transitionDuration));
  expect(duration).toBeLessThan(0.01);
});
