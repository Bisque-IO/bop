import { Link } from 'react-router-dom'
import { Cpu, Database, Layers, ArrowRight } from 'lucide-react'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'

interface CrateCardProps {
  id: string
  name: string
  tagline: string
  description: string
  icon: string
  version: string
}

const iconMap = {
  cpu: Cpu,
  database: Database,
  layers: Layers,
}

export function CrateCard({ id, name, tagline, description, icon, version }: CrateCardProps) {
  const Icon = iconMap[icon as keyof typeof iconMap] || Cpu

  return (
    <Card className="h-full hover:shadow-lg transition-all duration-300 hover:border-primary/50 group">
      <CardHeader>
        <div className="flex items-start justify-between">
          <div className="flex items-center space-x-3">
            <div className="p-3 bg-primary/10 rounded-lg group-hover:bg-primary/20 transition-colors">
              <Icon className="h-6 w-6 text-primary" />
            </div>
            <div>
              <CardTitle className="text-2xl">{name}</CardTitle>
              <Badge variant="secondary" className="mt-1">
                v{version}
              </Badge>
            </div>
          </div>
        </div>
        <CardDescription className="text-base pt-2">{tagline}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <p className="text-sm text-muted-foreground">{description}</p>
        <Button asChild className="w-full group-hover:translate-x-1 transition-transform">
          <Link to={`/${id}`}>
            Learn More
            <ArrowRight className="ml-2 h-4 w-4" />
          </Link>
        </Button>
      </CardContent>
    </Card>
  )
}
