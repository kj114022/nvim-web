use crate::backend::VfsBackend;
use anyhow::{Context, Result};
use std::path::Path;

/// Recursively remove a directory and its contents
/// Recursively remove a directory and its contents
pub async fn remove_dir_all(backend: &dyn VfsBackend, path: &str) -> Result<()> {
    boxed_remove_dir_all(backend, path.to_string()).await
}

fn boxed_remove_dir_all<'a>(
    backend: &'a dyn VfsBackend,
    path: String,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
    Box::pin(async move {
        if !backend.exists(&path).await? {
            return Ok(());
        }

        let stats = backend.stat(&path).await?;
        if !stats.is_dir {
            backend.remove_file(&path).await?;
            return Ok(());
        }

        let entries = backend.list(&path).await?;
        for entry in entries {
            let path_ref = Path::new(&path).join(&entry);
            let path_str = path_ref.to_str().context("Invalid path")?.to_string();
            boxed_remove_dir_all(backend, path_str).await?;
        }

        backend.remove_dir(&path).await?;
        Ok(())
    })
}

/// Recursively copy a directory and its contents
pub async fn copy_dir_all(
    backend: &dyn VfsBackend, 
    src: &str, 
    dest: &str
) -> Result<()> {
    let stats = backend.stat(src).await?;
    
    if stats.is_dir {
        if !backend.exists(dest).await? {
            backend.create_dir_all(dest).await?;
        }

        let entries = backend.list(src).await?;
        for entry in entries {
            let src_entry = Path::new(src).join(&entry);
            let dest_entry = Path::new(dest).join(&entry);
            
            let src_str = src_entry.to_str().context("Invalid src path")?;
            let dest_str = dest_entry.to_str().context("Invalid dest path")?;
            
            // Recursive copy
            // Note: Box::pin needed for async recursion if not using async-recursion crate
            // But here we are just defining the logic. To allow recursion in async fn,
            // we typically need the async_recursion crate or manual boxing.
            // Since we don't have async_recursion dep, we'll assume shallow depth or just implement it.
            // Actually, async fn recursion requires `Box::pin` or specific crate.
            // Let's use the explicit Box::pin approach for safety if compiler complains,
            // but for now let's try standard async fn and see if it compiles (modern rust handles some cases).
            // Correction: Rust async fn still doesn't support direct recursion without boxing.
            // I'll implement a helper that boxes.
            boxed_copy_dir_all(backend, src_str.to_string(), dest_str.to_string()).await?;
        }
    } else {
        // It's a file
         backend.copy(src, dest).await?;
    }
    
    Ok(())
}

use std::future::Future;
use std::pin::Pin;

fn boxed_copy_dir_all<'a>(
    backend: &'a dyn VfsBackend,
    src: String,
    dest: String,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
    Box::pin(async move {
        // Re-implement logic here to avoid circular dep with copy_dir_all
        // purely for the boxing wrapper
        
         let stats = backend.stat(&src).await?;
         if stats.is_dir {
            if !backend.exists(&dest).await? {
                backend.create_dir_all(&dest).await?;
            }

            let entries = backend.list(&src).await?;
            for entry in entries {
                let src_path = Path::new(&src).join(&entry);
                let dest_path = Path::new(&dest).join(&entry);
                
                boxed_copy_dir_all(
                    backend, 
                    src_path.to_str().unwrap().to_string(), 
                    dest_path.to_str().unwrap().to_string()
                ).await?;
            }
         } else {
             backend.copy(&src, &dest).await?;
         }
         Ok(())
    })
}
