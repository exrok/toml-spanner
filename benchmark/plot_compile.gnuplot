reset session
set terminal svg enhanced background rgb '#0D1117' size 780,300 font "Arial,14"

set xlabel __INSERT_LABEL_HERE__  tc rgb "white"  offset 0,graph 0.05
unset grid
unset ytics
set ytics scale 0 out nomirror  textcolor "white"
set xtics scale 0.75 out nomirror offset 0,graph 0.04 textcolor "white"

set border  lw 1 lc "grey"
set style fill solid 1.0
set lmargin 12

# Define colors: one per library
set linetype 1 lc rgb '#92B2CA'  # Blue
set linetype 2 lc rgb '#C0A7C7'  # Purple
set linetype 3 lc rgb '#E6A472'  # Yellow
set linetype 4 lc rgb '#D77C79'  # Red    -

$Data << EOD
__INSERT_DATA_HERE__
EOD

set yrange [0:*] reverse
set style fill solid
unset key

myBoxWidth = 0.8
set offsets 0,0,1.0-myBoxWidth/2.,1.0

plot $Data using (0.5*$2):0:(0.5*$2):(myBoxWidth/2.):($3+1):ytic(1) with boxxy lc var
