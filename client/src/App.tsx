import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { BrowserRouter } from 'react-router-dom'
import { ProviderWizardProvider } from '@/components/provider-wizard'
import { AccountGate } from '@/app/AccountGate'
import { AppLayout } from '@/app/AppLayout'

const queryClient = new QueryClient()

function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter basename={import.meta.env.BASE_URL}>
        <AccountGate>
          <ProviderWizardProvider>
            <AppLayout />
          </ProviderWizardProvider>
        </AccountGate>
      </BrowserRouter>
    </QueryClientProvider>
  )
}

export default App
