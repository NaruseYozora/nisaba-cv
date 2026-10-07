-- Named custom categories are peers of the four built-in categories.
-- Keep any existing materials accessible, but the old generic bucket is removable.
UPDATE categories SET builtin=0,revision=revision+1 WHERE id='builtin:custom';
DELETE FROM categories WHERE id='builtin:custom' AND name='自定义'
 AND NOT EXISTS (SELECT 1 FROM item_categories WHERE category_id='builtin:custom');
