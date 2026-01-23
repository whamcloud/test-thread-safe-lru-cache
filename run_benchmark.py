import subprocess
import sys
import re
import os

def run_benchmark():
    print("Compiling and running comparison benchmark... (this may take 2-3 minutes)")
    try:
        # Run the cargo command
        result = subprocess.run(
            ["cargo", "run", "--release", "--example", "benchmark_compare"],
            capture_output=True,
            text=True,
            cwd=os.path.dirname(os.path.abspath(__file__))
        )

        if result.returncode != 0:
            print("Error running benchmark:")
            print(result.stderr)
            return None

        # Parse output
        data = {} # { "ImplName": [(threads, ops), ...] }
        lines = result.stdout.splitlines()
        print("Benchmark raw output:")
        for line in lines:
            print(line)
            if "," in line and "Threads" not in line:
                parts = line.split(",")
                if len(parts) == 3:
                    try:
                        name = parts[0].strip()
                        threads = int(parts[1].strip())
                        ops = float(parts[2].strip())
                        
                        if name not in data:
                            data[name] = []
                        data[name].append((threads, ops))
                    except ValueError:
                        pass
        return data

    except Exception as e:
        print(f"Failed to execute benchmark: {e}")
        return None

def generate_html(data):
    if not data:
        print("No data to plot.")
        return

    # Extract labels (threads) from the first dataset
    first_impl = list(data.keys())[0]
    threads = [d[0] for d in data[first_impl]]
    
    datasets = []
    colors = [
        ('rgb(255, 99, 132)', 'rgba(255, 99, 132, 0.2)'), # Red
        ('rgb(54, 162, 235)', 'rgba(54, 162, 235, 0.2)'), # Blue
        ('rgb(255, 206, 86)', 'rgba(255, 206, 86, 0.2)'), # Yellow
        ('rgb(75, 192, 192)', 'rgba(75, 192, 192, 0.2)'), # Green
        ('rgb(153, 102, 255)', 'rgba(153, 102, 255, 0.2)'), # Purple
    ]
    
    for i, (name, points) in enumerate(data.items()):
        ops = [d[1] for d in points]
        color_idx = i % len(colors)
        datasets.append(f"""
                {{
                    label: '{name}',
                    data: {ops},
                    borderColor: '{colors[color_idx][0]}',
                    backgroundColor: '{colors[color_idx][1]}',
                    tension: 0.1,
                    fill: false,
                    pointRadius: 5
                }}""")

    datasets_str = ",\n".join(datasets)

    html_content = f"""
<!DOCTYPE html>
<html>
<head>
    <title>LRU Cache Comparison</title>
    <script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
    <style>
        body {{ font-family: sans-serif; padding: 20px; }}
        .container {{ width: 90%; margin: auto; }}
        h1 {{ text-align: center; }}
        .note {{ text-align: center; color: #666; font-size: 0.9em; }}
    </style>
</head>
<body>
    <div class="container">
        <h1>High-Performance Cache Comparison</h1>
        <p class="note">Workload: 90% GET, 10% PUT | 100k Items | 200k Keys</p>
        <canvas id="perfChart"></canvas>
    </div>

    <script>
        const ctx = document.getElementById('perfChart').getContext('2d');
        const perfChart = new Chart(ctx, {{
            type: 'line',
            data: {{
                labels: {threads},
                datasets: [
                    {datasets_str}
                ]
            }},
            options: {{
                responsive: true,
                interaction: {{
                    mode: 'index',
                    intersect: false,
                }},
                scales: {{
                    x: {{
                        title: {{ display: true, text: 'Number of Threads' }}
                    }},
                    y: {{
                        title: {{ display: true, text: 'Throughput (Ops/sec)' }},
                        beginAtZero: true
                    }}
                }}
            }}
        }});
    </script>
</body>
</html>
"""
    
    output_path = "benchmark_report.html"
    with open(output_path, "w") as f:
        f.write(html_content)
    
    print(f"\nSuccessfully generated report: {os.path.abspath(output_path)}")
    print("Open this file in your browser to see the performance graph.")

if __name__ == "__main__":
    data = run_benchmark()
    if data:
        generate_html(data)
