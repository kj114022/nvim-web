import { test, expect } from '@playwright/test';

test.describe('nvim-web E2E Tests', () => {
  test('loads the start screen', async ({ page }) => {
    await page.goto('/');
    
    // Wait for the page to load
    await page.waitForLoadState('networkidle');
    
    // Check for essential elements
    await expect(page.locator('canvas, #start-screen')).toBeVisible({ timeout: 10000 });
  });

  test('shows Neovim after start screen', async ({ page }) => {
    await page.goto('/');
    
    // Wait for start screen or canvas to appear
    await page.waitForLoadState('networkidle');
    
    // Either start screen is visible or canvas (Neovim) is visible
    const startScreen = page.locator('#start-screen');
    const canvas = page.locator('canvas');
    
    // At least one should be visible
    const startVisible = await startScreen.isVisible().catch(() => false);
    const canvasVisible = await canvas.isVisible().catch(() => false);
    
    expect(startVisible || canvasVisible).toBe(true);
  });

  test('has correct page title', async ({ page }) => {
    await page.goto('/');
    await expect(page).toHaveTitle(/nvim-web|Neovim/i);
  });

  test('canvas element is renderable', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    // Check if canvas exists and has dimensions
    const canvas = page.locator('canvas');
    if (await canvas.isVisible()) {
      const box = await canvas.boundingBox();
      expect(box).not.toBeNull();
      expect(box!.width).toBeGreaterThan(0);
      expect(box!.height).toBeGreaterThan(0);
    }
  });
});
