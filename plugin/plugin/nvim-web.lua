-- nvim-web plugin loader
if vim.g.loaded_nvim_web then return end
vim.g.loaded_nvim_web = true
require("nvim-web").setup({})
