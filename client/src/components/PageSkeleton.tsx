function PageSkeleton() {
  return (
    <div className="flex items-center justify-center h-[60vh]">
      <div className="space-y-3 text-center">
        <div className="mx-auto size-8 animate-spin rounded-full border-2 border-muted border-t-foreground" />
        <p className="text-sm text-muted-foreground">Loading…</p>
      </div>
    </div>
  )
}

export default PageSkeleton
